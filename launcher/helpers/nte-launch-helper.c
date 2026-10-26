#define UNICODE
#define _UNICODE
#include <windows.h>
#include <wchar.h>
#include <stdio.h>
#include <stdarg.h>

/*
 * 私有 Windows 启动桥接. direct 模式保持 Unreal 的参数专用命令行.
 * bridge 模式先挂起创建官方后台, 安装 UI 线程窗口守卫后恢复.
 * 守卫在主窗口显示请求生效前阻止映射, 登录和游戏窗口保持正常显示.
 *
 * Direct: NTE_GAME_EXE, NTE_GAME_CWD, NTE_GAME_ARGS
 * Bridge: NTE_BRIDGE_EXE, NTE_BRIDGE_CWD, NTE_BRIDGE_ARGS, NTE_SESSION_CLICK
 * Optional: NTE_HELPER_LOG
 */

static void fail(void) {
    ExitProcess(2);
}

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

typedef BOOL (WINAPI *GUARD_READY)(void);

static HMODULE load_window_guard(HOOKPROC *callback) {
    wchar_t path[32768], *name;
    static const wchar_t filename[] = L"nte-window-guard.dll";
    DWORD size = GetModuleFileNameW(NULL, path, 32768);
    HMODULE module;
    union { FARPROC raw; HOOKPROC hook; GUARD_READY ready; } address;
    if (!size || size >= 32768) fail();
    name = wcsrchr(path, L'\\');
    if (!name || (size_t)(name + 1 - path) + wcslen(filename) >= 32768) fail();
    wcscpy(name + 1, filename);
    module = LoadLibraryW(path);
    if (!module) { log_line(L"load window guard failed error=%lu", (unsigned long)GetLastError()); fail(); }
    address.raw = GetProcAddress(module, "OddityWindowGuardReady");
    if (!address.raw || !address.ready()) { FreeLibrary(module); fail(); }
    address.raw = GetProcAddress(module, "OddityWindowHook");
    if (!address.raw) { FreeLibrary(module); fail(); }
    *callback = address.hook;
    return module;
}

static int stop_child(HANDLE process) {
    DWORD code = STILL_ACTIVE;
    if (WaitForSingleObject(process, 0) == WAIT_OBJECT_0) return 1;
    if (!TerminateProcess(process, 2)) log_line(L"terminate child failed error=%lu", (unsigned long)GetLastError());
    if (WaitForSingleObject(process, 5000) != WAIT_OBJECT_0 ||
        !GetExitCodeProcess(process, &code) || code == STILL_ACTIVE) {
        log_line(L"child exit unconfirmed error=%lu", (unsigned long)GetLastError());
        return 0;
    }
    return 1;
}

static int run_and_optional_hide(wchar_t *exe, wchar_t *args, wchar_t *cwd, int hide) {
    STARTUPINFOW si;
    PROCESS_INFORMATION pi;
    HHOOK hook = NULL;
    HOOKPROC callback = NULL;
    HMODULE guard = NULL;
    DWORD code = 1;
    int failed = 0;
    ZeroMemory(&si, sizeof(si));
    si.cb = sizeof(si);
    ZeroMemory(&pi, sizeof(pi));
    if (hide) {
        guard = load_window_guard(&callback);
        si.dwFlags = STARTF_USESHOWWINDOW;
        si.wShowWindow = SW_HIDE;
    }
    if (!CreateProcessW(exe, args, NULL, NULL, TRUE, hide ? CREATE_SUSPENDED : 0,
                        NULL, cwd[0] ? cwd : NULL, &si, &pi)) fail();

    if (hide) {
        MSG message;
        PeekMessageW(&message, NULL, 0, 0, PM_NOREMOVE);
        hook = SetWindowsHookExW(WH_CBT, callback, guard, pi.dwThreadId);
        if (!hook || ResumeThread(pi.hThread) == (DWORD)-1) {
            log_line(L"install window guard or resume failed error=%lu", (unsigned long)GetLastError());
            failed = 1;
        } else {
            log_line(L"window guard installed pid=%lu thread=%lu", (unsigned long)pi.dwProcessId, (unsigned long)pi.dwThreadId);
            for (;;) {
                DWORD wait = MsgWaitForMultipleObjectsEx(1, &pi.hProcess, INFINITE, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
                if (wait == WAIT_OBJECT_0) break;
                if (wait != WAIT_OBJECT_0 + 1) { failed = 1; break; }
                while (PeekMessageW(&message, NULL, 0, 0, PM_REMOVE)) {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }
    } else if (WaitForSingleObject(pi.hProcess, INFINITE) != WAIT_OBJECT_0) failed = 1;

    if (failed) (void)stop_child(pi.hProcess);
    if (hook) UnhookWindowsHookEx(hook);
    if (guard) FreeLibrary(guard);
    if (!GetExitCodeProcess(pi.hProcess, &code) || code == STILL_ACTIVE) failed = 1;
    CloseHandle(pi.hThread);
    CloseHandle(pi.hProcess);
    return failed ? 2 : (int)code;
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
            wcscpy(args, L"/launcher /directly");
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
