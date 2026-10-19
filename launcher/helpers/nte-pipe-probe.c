#define UNICODE
#define _UNICODE
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdarg.h>

/*
 * nte-pipe-probe.exe — named-pipe observation helper for Neverness To Everness.
 *
 * Purpose: recover the launcher <-> game handoff protocol that normally runs
 * over \\.\PIPE\<ArcGameP> (a random 32-hex name in the game's Windows
 * environment block), WITHOUT injecting into, hooking, or modifying either
 * shipped binary. If ArcGameP is absent, the client falls back to
 * \\.\PIPE\one_and_global_merge_pipName, which is the probe's default.
 *
 * Two modes:
 *
 *   server  Create one or more named pipes and accept a connection. HTGame's
 *           WPGameClientSDK_64.dll connects to us instead of to NTEGame.exe,
 *           so we observe exactly what the game sends and when.
 *
 *   client  Connect to a pipe somebody else owns (normally NTEGame.exe) and
 *           replay a captured byte stream. This recovers the server's replies
 *           by replaying the real client's messages at it.
 *
 * Together the two modes yield a full bidirectional capture of the handoff.
 *
 * Configuration (environment variables, so a Linux-side launcher can drive it):
 *
 *   NTE_PROBE_MODE          "server" (default) or "client"
 *   NTE_PROBE_PIPES         comma-separated pipe names to create in server mode;
 *                           default is "\\.\PIPE\one_and_global_merge_pipName"
 *   NTE_PROBE_PIPE          single pipe name to connect to in client mode
 *   NTE_PROBE_IN            Windows path of a file whose bytes are sent after
 *                           a connection is established
 *   NTE_PROBE_IN_DELAY_MS   delay before that send (default 0)
 *   NTE_PROBE_IN_REPEAT     resend the payload every N ms (0 = once only)
 *   NTE_PROBE_OUT           Windows path of the capture log
 *   NTE_PROBE_DURATION_MS   stop after this long (0 = run until killed)
 *   NTE_PROBE_QUIET         "1" to suppress mirroring the log to stderr
 *
 * The log is line-oriented ASCII so it stays diffable and safe to commit:
 *
 *   <ms> EVENT detail...
 *   <ms> RX <n> bytes
 *   <ms>   <offset>  <hex bytes>  |ascii|
 *
 * It never writes anything back to the game except bytes the caller supplied,
 * and it never touches game files or the ACE anti-cheat.
 */

#define MAX_PIPES 16

static FILE *g_log = NULL;
static CRITICAL_SECTION g_lock;
static DWORD g_start = 0;
static int g_quiet = 0;

static DWORD env_u32(const wchar_t *name, DWORD fallback);
static DWORD env_str(const wchar_t *name, wchar_t *out, DWORD cap);
static DWORD launch_child(void);

static void log_open(const wchar_t *path)
{
    if (!path || !path[0]) {
        g_log = stderr;
        return;
    }
    g_log = _wfopen(path, L"wb");
    if (!g_log) {
        fprintf(stderr, "probe: cannot open log %ls\n", path);
        g_log = stderr;
    }
}

static void logf_(const char *fmt, ...)
{
    va_list ap;
    char line[2048];
    int n;

    EnterCriticalSection(&g_lock);

    n = _snprintf(line, sizeof(line) - 3, "%lu ", (unsigned long)(GetTickCount() - g_start));
    if (n < 0) n = 0;

    va_start(ap, fmt);
    _vsnprintf(line + n, sizeof(line) - n - 3, fmt, ap);
    va_end(ap);

    line[sizeof(line) - 3] = 0;
    strcat(line, "\n");

    if (g_log) { fputs(line, g_log); fflush(g_log); }
    if (!g_quiet) fputs(line, stderr);

    LeaveCriticalSection(&g_lock);
}

/* Hex + ASCII dump so a frame can be reconstructed byte-exactly later. */
static void hexdump(const char *tag, const unsigned char *p, DWORD n)
{
    DWORD i, j;
    char ascii[20];

    logf_("%s %lu bytes", tag, (unsigned long)n);

    for (i = 0; i < n; i += 16) {
        char hex[3 * 16 + 4];
        int h = 0;
        DWORD chunk = (n - i < 16) ? (n - i) : 16;

        for (j = 0; j < chunk; j++)
            h += _snprintf(hex + h, sizeof(hex) - h, "%02x ", p[i + j]);
        hex[h] = 0;

        for (j = 0; j < chunk; j++) {
            unsigned char c = p[i + j];
            ascii[j] = (c >= 0x20 && c < 0x7f) ? (char)c : '.';
        }
        ascii[chunk] = 0;

        logf_("  %04lx  %-48s |%s|", (unsigned long)i, hex, ascii);
    }
}

static unsigned char *read_file(const wchar_t *path, DWORD *out_len)
{
    HANDLE h;
    DWORD size, got = 0;
    unsigned char *buf;

    h = CreateFileW(path, GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL, NULL);
    if (h == INVALID_HANDLE_VALUE) {
        logf_("ERR cannot open input %ls (err=%lu)", path, GetLastError());
        return NULL;
    }

    size = GetFileSize(h, NULL);
    buf = (unsigned char *)malloc(size ? size : 1);
    if (buf && size) {
        if (!ReadFile(h, buf, size, &got, NULL)) {
            logf_("ERR read input failed (err=%lu)", GetLastError());
            free(buf);
            buf = NULL;
        }
    }
    CloseHandle(h);

    if (buf) {
        *out_len = got;
        logf_("loaded %lu bytes from %ls", (unsigned long)got, path);
    }
    return buf;
}

/*
 * Pumps bytes in both directions on one connected pipe handle.
 * `payload`/`payload_len` are optional bytes written once after `delay_ms`.
 */
static void pump(HANDLE h, const unsigned char *payload, DWORD payload_len,
                 DWORD delay_ms, DWORD repeat_ms)
{
    unsigned char buf[65536];
    DWORD n = 0, sent_total = 0;
    DWORD next_send = delay_ms;
    int payload_sent = 0;

    logf_("pump start on handle %p", h);

    for (;;) {
        DWORD avail = 0;

        if (!PeekNamedPipe(h, NULL, 0, NULL, &avail, NULL)) {
            logf_("pipe closed (PeekNamedPipe err=%lu)", GetLastError());
            break;
        }

        if (avail) {
            DWORD want = avail > sizeof(buf) ? (DWORD)sizeof(buf) : avail;
            if (!ReadFile(h, buf, want, &n, NULL)) {
                logf_("ReadFile failed (err=%lu)", GetLastError());
                break;
            }
            if (n) hexdump("RX", buf, n);
            continue;
        }

        if (payload && payload_len && !payload_sent &&
            (GetTickCount() - g_start) >= next_send) {
            if (WriteFile(h, payload, payload_len, &n, NULL)) {
                logf_("TX %lu bytes (scripted)", (unsigned long)n);
                hexdump("TX", payload, n);
                sent_total += n;
                payload_sent = 1;
                next_send = (GetTickCount() - g_start) + repeat_ms;
            } else {
                logf_("WriteFile failed (err=%lu)", GetLastError());
                break;
            }
        }

        if (payload && payload_len && repeat_ms && payload_sent &&
            (GetTickCount() - g_start) >= next_send) {
            if (WriteFile(h, payload, payload_len, &n, NULL)) {
                logf_("TX %lu bytes (repeat)", (unsigned long)n);
                hexdump("TX", payload, n);
                sent_total += n;
                next_send = (GetTickCount() - g_start) + repeat_ms;
            } else {
                logf_("WriteFile failed (err=%lu)", GetLastError());
                break;
            }
        }

        Sleep(20);
    }

    logf_("pump end, sent_total=%lu", (unsigned long)sent_total);
}

/*
 * Optionally start the game ourselves so that the pipe server and the game
 * share one Wine session (and therefore one named-pipe namespace).
 * Uses CreateProcessW with lpApplicationName set, which is also how the
 * shipped NTEGame.exe avoids leaking the exe path into Unreal's map URL.
 */
static DWORD launch_child(void)
{
    wchar_t exe[1024], cwd[1024], args[2048];
    STARTUPINFOW si;
    PROCESS_INFORMATION pi;

    if (!env_str(L"NTE_PROBE_LAUNCH_EXE", exe, 1024)) return 0;

    if (!env_str(L"NTE_PROBE_LAUNCH_CWD", cwd, 1024)) cwd[0] = 0;
    if (!env_str(L"NTE_PROBE_LAUNCH_ARGS", args, 2048))
        wcscpy(args, L"/Game/LoginAndCreate/Map/Updater/Updater_P -SAVEWINPOS=1");

    ZeroMemory(&si, sizeof(si));
    si.cb = sizeof(si);
    ZeroMemory(&pi, sizeof(pi));

    if (!CreateProcessW(exe, args, NULL, NULL, TRUE, 0, NULL,
                        cwd[0] ? cwd : NULL, &si, &pi)) {
        logf_("ERR CreateProcessW(%ls) failed err=%lu", exe, GetLastError());
        return 0;
    }

    logf_("LAUNCHED %ls pid=%lu", exe, (unsigned long)pi.dwProcessId);
    CloseHandle(pi.hThread);
    CloseHandle(pi.hProcess);
    return pi.dwProcessId;
}

/*
 * Create pipes whose names embed the launched child's pid, e.g.
 * "\\.\PIPE\QQGameProcPipeGameReceive%d". The shipped launcher knows the game
 * pid because it spawned it, so a pid-keyed channel can be created before the
 * game connects without any extra handshake. NTE_PROBE_PID_PIPES is a
 * comma-separated list of such templates.
 */
static int add_pid_pipes(char *names[], char (*storage)[512], int count, int cap,
                         DWORD pid, char *templates)
{
    char *p = templates;

    if (!pid) return count;

    while (p && *p && count < cap) {
        char *tok = p;
        p = strchr(p, ',');
        if (p) { *p = 0; p++; }
        while (*tok == ' ') tok++;
        if (!*tok) continue;

        _snprintf(storage[count], 511, tok, (int)pid);
        storage[count][511] = 0;
        names[count] = storage[count];
        logf_("pid pipe template '%s' -> %s", tok, names[count]);
        count++;
    }
    return count;
}

static void run_server(char *names[], int count, const unsigned char *payload,
                       DWORD payload_len, DWORD delay_ms, DWORD repeat_ms,
                       DWORD duration_ms)
{
    HANDLE pipes[MAX_PIPES];
    int i;
    DWORD pid;
    wchar_t tmpl[2048];
    static char pid_storage[MAX_PIPES][512];

    if (count > MAX_PIPES) count = MAX_PIPES;
    for (i = 0; i < MAX_PIPES; i++) pipes[i] = INVALID_HANDLE_VALUE;

    for (i = 0; i < count; i++) {
        wchar_t wname[512];
        MultiByteToWideChar(CP_ACP, 0, names[i], -1, wname, 512);

        pipes[i] = CreateNamedPipeW(
            wname,
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            1 << 16, 1 << 16, 0, NULL);

        if (pipes[i] == INVALID_HANDLE_VALUE) {
            logf_("ERR CreateNamedPipeW(%s) failed err=%lu%s", names[i], GetLastError(),
                  GetLastError() == ERROR_ACCESS_DENIED
                      ? " (name already owned - is NTEGame.exe still running?)" : "");
        } else {
            logf_("created pipe %s", names[i]);
        }
    }

    /* Named pipes now exist, so the game cannot race us. */
    pid = launch_child();

    if (pid && env_str(L"NTE_PROBE_PID_PIPES", tmpl, 2048)) {
        char raw[2048];
        WideCharToMultiByte(CP_ACP, 0, tmpl, -1, raw, 2048, NULL, NULL);
        count = add_pid_pipes(names, pid_storage, count, MAX_PIPES, pid, raw);

        for (i = 0; i < count; i++) {
            wchar_t wname[512];
            MultiByteToWideChar(CP_ACP, 0, names[i], -1, wname, 512);
            if (pipes[i] != INVALID_HANDLE_VALUE) continue;
            pipes[i] = CreateNamedPipeW(
                wname,
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                1 << 16, 1 << 16, 0, NULL);
            if (pipes[i] == INVALID_HANDLE_VALUE)
                logf_("ERR CreateNamedPipeW(%s) failed err=%lu", names[i], GetLastError());
            else
                logf_("created pipe %s", names[i]);
        }
    }

    for (;;) {
        DWORD now = GetTickCount() - g_start;
        if (duration_ms && now >= duration_ms) {
            logf_("duration reached, exiting server loop");
            break;
        }

        for (i = 0; i < count; i++) {
            DWORD client_pid = 0;
            BOOL ok;

            if (pipes[i] == INVALID_HANDLE_VALUE) continue;

            ok = ConnectNamedPipe(pipes[i], NULL);
            if (!ok && GetLastError() != ERROR_PIPE_CONNECTED) {
                /* Nobody waiting yet: keep polling. */
                continue;
            }

            client_pid = 0;
            {
                typedef BOOL (WINAPI *pfn_GetNamedPipeClientProcessId)(HANDLE, PULONG);
                HMODULE k = GetModuleHandleW(L"kernel32.dll");
                pfn_GetNamedPipeClientProcessId fn = k ? (pfn_GetNamedPipeClientProcessId)
                    (void *)GetProcAddress(k, "GetNamedPipeClientProcessId") : NULL;
                ULONG pid = 0;
                if (fn && fn(pipes[i], &pid)) client_pid = pid;
            }

            logf_("CONNECT on %s client_pid=%lu", names[i], (unsigned long)client_pid);

            pump(pipes[i], payload, payload_len, delay_ms, repeat_ms);

            DisconnectNamedPipe(pipes[i]);
            logf_("disconnected %s", names[i]);
        }

        Sleep(50);
    }
}

static void run_client(const char *name, const unsigned char *payload,
                       DWORD payload_len, DWORD delay_ms, DWORD repeat_ms,
                       DWORD duration_ms)
{
    wchar_t wname[512];
    HANDLE h;
    DWORD deadline;

    MultiByteToWideChar(CP_ACP, 0, name, -1, wname, 512);

    logf_("waiting for pipe %s", name);

    deadline = duration_ms ? duration_ms : 60000;
    while ((GetTickCount() - g_start) < deadline) {
        if (WaitNamedPipeW(wname, 2000)) break;
        logf_("  WaitNamedPipeW err=%lu, retrying", GetLastError());
        Sleep(200);
    }

    h = CreateFileW(wname, GENERIC_READ | GENERIC_WRITE, 0, NULL, OPEN_EXISTING, 0, NULL);
    if (h == INVALID_HANDLE_VALUE) {
        logf_("ERR CreateFileW(%s) err=%lu", name, GetLastError());
        return;
    }
    logf_("connected to %s", name);

    {
        DWORD mode = PIPE_READMODE_MESSAGE;
        if (!SetNamedPipeHandleState(h, &mode, NULL, NULL))
            logf_("ERR SetNamedPipeHandleState err=%lu", GetLastError());
        else
            logf_("message read mode set");
    }

    pump(h, payload, payload_len, delay_ms, repeat_ms);
    CloseHandle(h);
}

static DWORD env_u32(const wchar_t *name, DWORD fallback)
{
    wchar_t buf[64];
    DWORD n = GetEnvironmentVariableW(name, buf, 64);
    if (n == 0 || n >= 64) return fallback;
    return (DWORD)wcstoul(buf, NULL, 10);
}

static DWORD env_str(const wchar_t *name, wchar_t *out, DWORD cap)
{
    DWORD n = GetEnvironmentVariableW(name, out, cap);
    if (n == 0 || n >= cap) { out[0] = 0; return 0; }
    return n;
}

int WINAPI WinMain(HINSTANCE inst, HINSTANCE prev, LPSTR cmdline, int show)
{
    wchar_t mode[64], pipes_raw[2048], single[512], in_path[1024], out_path[1024];
    char names_ansi[MAX_PIPES][512];
    char *names[MAX_PIPES];
    int count = 0;
    unsigned char *payload = NULL;
    DWORD payload_len = 0, delay_ms, repeat_ms, duration_ms;
    char *p, *tok;

    (void)inst; (void)prev; (void)cmdline; (void)show;

    InitializeCriticalSection(&g_lock);
    g_start = GetTickCount();

    env_str(L"NTE_PROBE_MODE", mode, 64);
    if (!mode[0]) wcscpy(mode, L"server");

    env_str(L"NTE_PROBE_OUT", out_path, 1024);
    log_open(out_path);

    g_quiet = env_u32(L"NTE_PROBE_QUIET", 0) != 0;

    delay_ms    = env_u32(L"NTE_PROBE_IN_DELAY_MS", 0);
    repeat_ms   = env_u32(L"NTE_PROBE_IN_REPEAT", 0);
    duration_ms = env_u32(L"NTE_PROBE_DURATION_MS", 0);

    if (env_str(L"NTE_PROBE_IN", in_path, 1024))
        payload = read_file(in_path, &payload_len);

    logf_("=== nte-pipe-probe mode=%ls delay=%lu repeat=%lu duration=%lu ===",
          mode, (unsigned long)delay_ms, (unsigned long)repeat_ms,
          (unsigned long)duration_ms);

    if (!_wcsicmp(mode, L"list")) {
        WIN32_FIND_DATAW fd;
        HANDLE f = FindFirstFileW(L"\\\\.\\PIPE\\*", &fd);
        if (f == INVALID_HANDLE_VALUE) {
            logf_("ERR FindFirstFileW pipe list failed err=%lu", GetLastError());
            return 2;
        }
        do {
            logf_("PIPE %ls", fd.cFileName);
        } while (FindNextFileW(f, &fd));
        FindClose(f);
        return 0;
    }

    if (!_wcsicmp(mode, L"client")) {
        if (!env_str(L"NTE_PROBE_PIPE", single, 512)) {
            logf_("ERR NTE_PROBE_PIPE required in client mode");
            return 2;
        }
        {
            char ansi[512];
            WideCharToMultiByte(CP_ACP, 0, single, -1, ansi, 512, NULL, NULL);
            run_client(ansi, payload, payload_len, delay_ms, repeat_ms, duration_ms);
        }
        return 0;
    }

    if (!env_str(L"NTE_PROBE_PIPES", pipes_raw, 2048))
        wcscpy(pipes_raw, L"\\\\.\\PIPE\\one_and_global_merge_pipName");

    {
        char ansi[2048];
        WideCharToMultiByte(CP_ACP, 0, pipes_raw, -1, ansi, 2048, NULL, NULL);

        p = ansi;
        while (p && *p && count < MAX_PIPES) {
            tok = p;
            p = strchr(p, ',');
            if (p) { *p = 0; p++; }
            while (*tok == ' ') tok++;
            strncpy(names_ansi[count], tok, 511);
            names_ansi[count][511] = 0;
            names[count] = names_ansi[count];
            count++;
        }
    }

    run_server(names, count, payload, payload_len, delay_ms, repeat_ms, duration_ms);

    if (payload) free(payload);
    if (g_log && g_log != stderr) fclose(g_log);
    return 0;
}