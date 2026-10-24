#define UNICODE
#define _UNICODE
#include <windows.h>
#include <wchar.h>
#include <stdio.h>
#include <stdarg.h>

/*
 * 私有 Windows 启动桥接. direct 模式保持 Unreal 的参数专用命令行.
 * bridge 模式先挂起创建官方后台, 安装窗口事件钩子后恢复.
 * 回调仅处理该 PID; 启动时请求 SW_HIDE, 后续 CREATE/SHOW 事件隐藏启动器主窗口;
 * 登录遮罩 ToolSaveBitsOwnDC 和游戏进程的窗口保持正常显示.
 *
 * Direct: NTE_GAME_EXE, NTE_GAME_CWD, NTE_GAME_ARGS
 * Bridge: NTE_BRIDGE_EXE, NTE_BRIDGE_CWD, NTE_BRIDGE_ARGS
 * Optional: NTE_HELPER_LOG
 */

static void fail(void) {
    ExitProcess(2);
}

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
    /* 官方自动登录遮罩由游戏窗口拥有, 必须保留. */
    return wcsstr(cls, L"ToolSaveBitsOwnDC") != NULL;
}

static int is_ignored_class(const wchar_t *cls) {
    return wcsstr(cls, L"TrayIcon") != NULL ||
           wcsstr(cls, L"IME") != NULL ||
           wcsstr(cls, L"WineTablet") != NULL;
}

static void hide_launcher_window(HWND hwnd) {
    DWORD pid = 0;
    wchar_t cls[256] = L"";
    wchar_t title[512] = L"";
    HWND owner;

    if (!hwnd || !IsWindow(hwnd)) return;
    GetWindowThreadProcessId(hwnd, &pid);
    if (pid != g_launcher_pid || GetAncestor(hwnd, GA_ROOT) != hwnd ||
        (GetWindowLongPtrW(hwnd, GWL_STYLE) & WS_CHILD)) return;
    if (!GetClassNameW(hwnd, cls, 256)) return;
    if (is_ignored_class(cls) || is_cover_class(cls)) return;
    GetWindowTextW(hwnd, title, 512);
    owner = GetWindow(hwnd, GW_OWNER);
    if (wcsstr(cls, L"QWindow") != NULL || wcsstr(title, L"NTEGame") != NULL || owner != NULL) {
        if (IsWindowVisible(hwnd)) {
            ShowWindow(hwnd, SW_HIDE);
            log_line(L"hide pid=%lu hwnd=%p cls=%ls", (unsigned long)pid, (void *)hwnd, cls);
        }
    }
}

static void CALLBACK window_event(HWINEVENTHOOK hook, DWORD event, HWND hwnd,
                                  LONG object, LONG child, DWORD thread, DWORD time) {
    (void)hook;
    (void)thread;
    (void)time;
    if ((event == EVENT_OBJECT_CREATE || event == EVENT_OBJECT_SHOW) &&
        object == OBJID_WINDOW && child == CHILDID_SELF) hide_launcher_window(hwnd);
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
    HWINEVENTHOOK hook = NULL;
    DWORD code = 1;
    int failed = 0;
    ZeroMemory(&si, sizeof(si));
    si.cb = sizeof(si);
    ZeroMemory(&pi, sizeof(pi));
    if (hide) {
        si.dwFlags = STARTF_USESHOWWINDOW;
        si.wShowWindow = SW_HIDE;
    }
    if (!CreateProcessW(exe, args, NULL, NULL, TRUE, hide ? CREATE_SUSPENDED : 0,
                        NULL, cwd[0] ? cwd : NULL, &si, &pi)) fail();

    if (hide) {
        MSG message;
        /* 先建立事件线程的消息队列, 防止恢复 child 前漏掉初始窗口事件. */
        PeekMessageW(&message, NULL, 0, 0, PM_NOREMOVE);
        g_launcher_pid = pi.dwProcessId;
        /* Wine 在挂起进程注册时需要全局订阅; 回调只处理持有句柄的本轮 PID. */
        hook = SetWinEventHook(EVENT_OBJECT_CREATE, EVENT_OBJECT_SHOW, NULL, window_event,
                              0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS);
        if (!hook || ResumeThread(pi.hThread) == (DWORD)-1) {
            log_line(L"install window hook or resume failed error=%lu", (unsigned long)GetLastError());
            failed = 1;
        } else {
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
    if (hook) UnhookWinEvent(hook);
    g_launcher_pid = 0;
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
