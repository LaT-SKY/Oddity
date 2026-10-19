#define UNICODE
#define _UNICODE
#include <windows.h>
#include <wchar.h>
#include <stdio.h>
#include <stdarg.h>

/*
 * Tiny Windows launcher for NTE.
 *
 * Direct HTGame mode:
 *   Wine/umu starts a process with the executable path as argv[0]; Unreal
 *   parses that as the map URL. CreateProcessW(lpApplicationName=exe,
 *   lpCommandLine="/Game/...") avoids it.
 *
 * Background bridge mode:
 *   Start NTEGame.exe /launcher /directly /autoplay and keep its launcher
 *   window out of the way. NTEGame still performs auto-login and the pipe
 *   handoff; the game window and the newautologin.qml cover stay visible.
 *
 *   We do not hide "all windows once after N seconds" any more. The launcher
 *   creates/re-shows windows at several points (main Qt window, CEF host,
 *   game-cover window). A monitor thread hides only the launcher's own
 *   windows and explicitly preserves the game-cover window, which Qt creates
 *   later with class "...QWindowToolSaveBitsOwnDC" and owner = HTGame window.
 *
 * Environment:
 *   Direct:   NTE_GAME_EXE, NTE_GAME_CWD, NTE_GAME_ARGS
 *   Bridge:   NTE_BRIDGE_EXE, NTE_BRIDGE_CWD, NTE_BRIDGE_ARGS
 *   Optional: NTE_HELPER_LOG  (Windows path; one line per hidden window)
 */

static void fail(void) {
    ExitProcess(2);
}

static volatile LONG g_stop = 0;
static DWORD g_launcher_pid = 0;
static FILE *g_log = NULL;

static void log_line(const wchar_t *fmt, ...) {
    wchar_t buf[1024];
    va_list ap;
    if (!g_log) return;
    va_start(ap, fmt);
    _vsnwprintf(buf, 1023, fmt, ap);
    va_end(ap);
    buf[1023] = 0;
    fwprintf(g_log, L"%ls\n", buf);
    fflush(g_log);
}

static int is_cover_class(const wchar_t *cls) {
    /* The auto-login/game cover is Qt51517QWindowToolSaveBitsOwnDC.
     * It is owned by the HTGame window and must stay visible. */
    return wcsstr(cls, L"ToolSaveBitsOwnDC") != NULL;
}

static int is_ignored_class(const wchar_t *cls) {
    return wcsstr(cls, L"TrayIcon") != NULL ||
           wcsstr(cls, L"IME") != NULL ||
           wcsstr(cls, L"WineTablet") != NULL;
}

static BOOL CALLBACK hide_proc(HWND hwnd, LPARAM lp) {
    DWORD pid = 0;
    wchar_t cls[256] = L"";
    wchar_t title[512] = L"";
    HWND owner;
    (void)lp;

    GetWindowThreadProcessId(hwnd, &pid);
    if (pid != g_launcher_pid) return TRUE;
    if (!IsWindowVisible(hwnd)) return TRUE;

    GetClassNameW(hwnd, cls, 255);
    GetWindowTextW(hwnd, title, 511);

    if (is_ignored_class(cls)) return TRUE;
    if (is_cover_class(cls)) return TRUE;

    /* Hide the launcher's Qt/CEF top-level windows. Keep the cover and the
     * actual game window (different pid) untouched. */
    owner = GetWindow(hwnd, GW_OWNER);
    if (wcsstr(cls, L"QWindow") != NULL || wcsstr(title, L"NTEGame") != NULL ||
        owner != NULL) {
        ShowWindow(hwnd, SW_HIDE);
        log_line(L"hide pid=%lu hwnd=%p cls=%ls title=%ls", (unsigned long)pid,
                 (void *)hwnd, cls, title);
    }
    return TRUE;
}

static DWORD WINAPI monitor_thread(LPVOID param) {
    (void)param;
    while (InterlockedCompareExchange(&g_stop, 0, 0) == 0) {
        EnumWindows(hide_proc, 0);
        Sleep(200);
    }
    return 0;
}

static int run_and_optional_hide(wchar_t *exe, wchar_t *args, wchar_t *cwd, int hide) {
    STARTUPINFOW si;
    PROCESS_INFORMATION pi;
    HANDLE monitor = NULL;
    ZeroMemory(&si, sizeof(si));
    si.cb = sizeof(si);
    ZeroMemory(&pi, sizeof(pi));

    if (!CreateProcessW(exe, args, NULL, NULL, TRUE, 0, NULL,
                        cwd[0] ? cwd : NULL, &si, &pi)) {
        fail();
    }

    if (hide) {
        g_launcher_pid = pi.dwProcessId;
        InterlockedExchange(&g_stop, 0);
        monitor = CreateThread(NULL, 0, monitor_thread, NULL, 0, NULL);
    }

    WaitForSingleObject(pi.hProcess, INFINITE);

    if (monitor) {
        InterlockedExchange(&g_stop, 1);
        WaitForSingleObject(monitor, 2000);
        CloseHandle(monitor);
    }

    DWORD code = 1;
    GetExitCodeProcess(pi.hProcess, &code);
    CloseHandle(pi.hThread);
    CloseHandle(pi.hProcess);
    return (int)code;
}

int WINAPI WinMain(HINSTANCE h, HINSTANCE p, LPSTR cmd, int show) {
    (void)h;
    (void)p;
    (void)cmd;
    (void)show;

    wchar_t exe[32768];
    wchar_t cwd[32768];
    wchar_t args[65536];
    wchar_t log_path[1024];
    DWORD n;

    if (GetEnvironmentVariableW(L"NTE_HELPER_LOG", log_path, 1024) &&
        log_path[0]) {
        g_log = _wfopen(log_path, L"a+");
    }

    n = GetEnvironmentVariableW(L"NTE_BRIDGE_EXE", exe, 32768);
    if (n > 0 && n < 32768) {
        n = GetEnvironmentVariableW(L"NTE_BRIDGE_CWD", cwd, 32768);
        if (n == 0 || n >= 32768) cwd[0] = 0;
        n = GetEnvironmentVariableW(L"NTE_BRIDGE_ARGS", args, 65536);
        if (n == 0 || n >= 65536) {
            wcscpy(args, L"/launcher /directly /autoplay");
        }
        return run_and_optional_hide(exe, args, cwd, 1);
    }

    n = GetEnvironmentVariableW(L"NTE_GAME_EXE", exe, 32768);
    if (n == 0 || n >= 32768) fail();

    n = GetEnvironmentVariableW(L"NTE_GAME_CWD", cwd, 32768);
    if (n == 0 || n >= 32768) cwd[0] = 0;

    n = GetEnvironmentVariableW(L"NTE_GAME_ARGS", args, 65536);
    if (n == 0 || n >= 65536) {
        wcscpy(args, L"/Game/LoginAndCreate/Map/Updater/Updater_P -SAVEWINPOS=1");
    }

    return run_and_optional_hide(exe, args, cwd, 0);
}
