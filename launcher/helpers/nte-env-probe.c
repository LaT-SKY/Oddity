#define UNICODE
#define _UNICODE
#include <windows.h>
#include <tlhelp32.h>
#include <stdio.h>
#include <stdlib.h>
#include <wchar.h>
#include <stdarg.h>

/*
 * Read-only Windows environment probe for another process in the same Wine
 * prefix. It opens the target process, walks PEB -> ProcessParameters ->
 * Environment, and prints a small whitelist of ArcGame* / launcher variables.
 * It does not inject, write, suspend, or modify the target.
 *
 * Usage: nte-env-probe.exe [pid]
 *   without pid: find HTGame.exe / GameThread in the process snapshot.
 * Env: NTE_ENV_OUT = Windows path for the log (optional).
 */

typedef struct _MY_PBI {
    PVOID Reserved1;
    PVOID PebBaseAddress;
    PVOID Reserved2[2];
    ULONG_PTR UniqueProcessId;
    PVOID Reserved3;
} MY_PBI;

typedef LONG (WINAPI *pfn_NtQueryInformationProcess)(HANDLE, ULONG, PVOID, ULONG, PULONG);

static FILE *g_log;

static void logf_(const char *fmt, ...)
{
    va_list ap;
    char line[2048];
    va_start(ap, fmt);
    _vsnprintf(line, sizeof(line) - 2, fmt, ap);
    va_end(ap);
    line[sizeof(line) - 2] = 0;
    strcat(line, "\n");
    if (g_log) { fputs(line, g_log); fflush(g_log); }
    fputs(line, stderr);
}

static DWORD find_target(void)
{
    HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    PROCESSENTRY32W pe;
    DWORD found = 0;
    if (snap == INVALID_HANDLE_VALUE) return 0;
    pe.dwSize = sizeof(pe);
    if (Process32FirstW(snap, &pe)) {
        do {
            if (wcsstr(pe.szExeFile, L"HTGame") || wcsstr(pe.szExeFile, L"GameThread")) {
                found = pe.th32ProcessID;
                logf_("found target pid=%lu exe=%ls", (unsigned long)found, pe.szExeFile);
                break;
            }
        } while (Process32NextW(snap, &pe));
    }
    CloseHandle(snap);
    return found;
}

static int read_mem(HANDLE h, ULONGLONG addr, void *buf, SIZE_T size)
{
    SIZE_T got = 0;
    if (!ReadProcessMemory(h, (LPCVOID)(ULONG_PTR)addr, buf, size, &got)) return 0;
    return got == size;
}

static void dump_env(HANDLE h)
{
    HMODULE ntdll = GetModuleHandleW(L"ntdll.dll");
    pfn_NtQueryInformationProcess NtQIP = ntdll ? (pfn_NtQueryInformationProcess)
        (void *)GetProcAddress(ntdll, "NtQueryInformationProcess") : NULL;
    MY_PBI pbi;
    ULONG ret = 0;
    ULONGLONG peb = 0, params = 0, env = 0, env_len = 0;
    wchar_t *block;
    SIZE_T cap = 256 * 1024, i;
    int printed = 0;

    if (!NtQIP) { logf_("ERR no NtQueryInformationProcess"); return; }
    ZeroMemory(&pbi, sizeof(pbi));
    if (NtQIP(h, 0 /* ProcessBasicInformation */, &pbi, sizeof(pbi), &ret) != 0) {
        logf_("ERR NtQueryInformationProcess failed");
        return;
    }
    peb = (ULONGLONG)(ULONG_PTR)pbi.PebBaseAddress;
    logf_("PEB=%p", (void *)(ULONG_PTR)peb);
    if (!peb || !read_mem(h, peb + 0x20, &params, sizeof(params))) {
        logf_("ERR read ProcessParameters");
        return;
    }
    logf_("ProcessParameters=%p", (void *)(ULONG_PTR)params);
    if (!params || !read_mem(h, params + 0x80, &env, sizeof(env))) {
        logf_("ERR read Environment ptr");
        return;
    }
    logf_("Environment=%p", (void *)(ULONG_PTR)env);
    if (!env) return;

    block = (wchar_t *)malloc(cap);
    if (!block) { logf_("ERR malloc"); return; }
    ZeroMemory(block, cap);
    if (!ReadProcessMemory(h, (LPCVOID)(ULONG_PTR)env, block, cap, &env_len)) {
        /* Retry in 16 KiB pieces until the first failure. */
        SIZE_T off = 0;
        while (off < cap) {
            SIZE_T chunk = 16 * 1024, got = 0;
            if (off + chunk > cap) chunk = cap - off;
            if (!ReadProcessMemory(h, (LPCVOID)(ULONG_PTR)(env + off), (BYTE *)block + off, chunk, &got))
                break;
            off += got;
            if (got < chunk) break;
        }
        env_len = off;
    }
    logf_("environment bytes read=%lu", (unsigned long)env_len);

    for (i = 0; i + 2 < env_len / sizeof(wchar_t); ) {
        wchar_t *s = block + i;
        SIZE_T len = wcslen(s);
        wchar_t *eq;
        if (len == 0) break;
        eq = wcschr(s, L'=');
        if (eq) {
            wchar_t key[128];
            SIZE_T klen = (SIZE_T)(eq - s);
            if (klen >= 128) klen = 127;
            wmemcpy(key, s, klen);
            key[klen] = 0;
            if (!_wcsicmp(key, L"ArcGameP") || !_wcsicmp(key, L"ArcGameAppId") ||
                !_wcsicmp(key, L"ArcGameSDKSharePath") || !_wcsicmp(key, L"ArcQuitType") ||
                !_wcsicmp(key, L"LauncherStarted") || !_wcsicmp(key, L"pgp_scale_value") ||
                !_wcsicmp(key, L"ArcDataIndex") || !_wcsicmp(key, L"sdkLogPath")) {
                logf_("ENV %ls=%ls", key, eq + 1);
                printed++;
            } else if (!_wcsicmp(key, L"ArcGameAppKey") || !_wcsicmp(key, L"ArcGameMsgSkipFlag")) {
                logf_("ENV %ls=<len %lu>", key, (unsigned long)wcslen(eq + 1));
                printed++;
            }
        }
        i += len + 1;
    }
    free(block);
    logf_("whitelist entries=%d", printed);
}

int WINAPI WinMain(HINSTANCE inst, HINSTANCE prev, LPSTR cmdline, int show)
{
    wchar_t out[1024];
    DWORD pid = 0;
    HANDLE h;
    (void)inst; (void)prev; (void)cmdline; (void)show;

    if (GetEnvironmentVariableW(L"NTE_ENV_OUT", out, 1024) && out[0]) {
        g_log = _wfopen(out, L"wb");
    }
    if (!g_log) g_log = stderr;

    pid = GetEnvironmentVariableW(L"NTE_TARGET_PID", out, 32) ? (DWORD)wcstoul(out, NULL, 10) : 0;
    if (!pid) pid = find_target();
    if (!pid) { logf_("ERR target not found"); return 2; }

    h = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, pid);
    if (!h) {
        logf_("ERR OpenProcess(%lu) failed err=%lu", (unsigned long)pid, GetLastError());
        return 3;
    }
    dump_env(h);
    CloseHandle(h);
    if (g_log && g_log != stderr) fclose(g_log);
    return 0;
}
