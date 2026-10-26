#include "../nte-window-guard.c"
#include <assert.h>
#include <stdio.h>

static unsigned int main_shows;

static LRESULT CALLBACK test_proc(HWND window, UINT message, WPARAM wparam, LPARAM lparam)
{
    if (message == WM_WINDOWPOSCHANGED && (((WINDOWPOS *)lparam)->flags & SWP_SHOWWINDOW)) {
        wchar_t title[32];
        GetWindowTextW(window, title, 32);
        if (!wcscmp(title, L"main")) ++main_shows;
    }
    return DefWindowProcW(window, message, wparam, lparam);
}

static HWND test_window(HINSTANCE instance, const wchar_t *cls, const wchar_t *title,
                        int width, int height, int visible)
{
    WNDCLASSW klass;
    ZeroMemory(&klass, sizeof(klass));
    klass.lpfnWndProc = test_proc;
    klass.hInstance = instance;
    klass.lpszClassName = cls;
    RegisterClassW(&klass);
    return CreateWindowW(cls, title, WS_POPUP | (visible ? WS_VISIBLE : 0),
                         0, 0, width, height, NULL, NULL, instance, NULL);
}

static DWORD WINAPI other_thread(LPVOID unused)
{
    HWND game;
    (void)unused;
    game = test_window(GetModuleHandleW(NULL), L"Qt51517QWindowOwnDC", L"game", 1280, 720, 1);
    assert(game && IsWindowVisible(game));
    DestroyWindow(game);
    return 0;
}

int main(void)
{
    HINSTANCE instance = GetModuleHandleW(NULL);
    HWND main_window, resized, login, cover, other;
    HANDLE thread;
    HHOOK hook;
    SetEnvironmentVariableW(L"NTE_SESSION_CLICK", L"1280,720,1120,636");
    assert(OddityWindowGuardReady());
    hook = SetWindowsHookExW(WH_CBT, OddityWindowHook, NULL, GetCurrentThreadId());
    assert(hook);
    main_window = test_window(instance, L"Qt51517QWindowOwnDC", L"main", 1280, 720, 1);
    resized = test_window(instance, L"Qt51517QWindowOwnDC", L"main", 1, 1, 0);
    login = test_window(instance, L"Qt51517QWindowOwnDC", L"login", 1200, 750, 1);
    cover = test_window(instance, L"Qt51517QWindowToolSaveBitsOwnDC", L"cover", 1280, 720, 1);
    other = test_window(instance, L"OtherWindow", L"other", 1280, 720, 1);
    assert(main_window && resized && login && cover && other);
    assert(!IsWindowVisible(main_window));
    SetWindowPos(resized, NULL, 0, 0, 1280, 720, SWP_SHOWWINDOW | SWP_NOZORDER);
    assert(!IsWindowVisible(resized));
    SetWindowPos(main_window, NULL, 0, 0, 640, 480, SWP_SHOWWINDOW | SWP_NOZORDER);
    ShowWindow(main_window, SW_SHOW);
    assert(!IsWindowVisible(main_window));
    assert(main_shows == 0);
    assert(IsWindowVisible(login));
    assert(IsWindowVisible(cover));
    assert(IsWindowVisible(other));
    thread = CreateThread(NULL, 0, other_thread, NULL, 0, NULL);
    assert(thread && WaitForSingleObject(thread, 5000) == WAIT_OBJECT_0);
    CloseHandle(thread);
    DestroyWindow(main_window);
    DestroyWindow(resized);
    DestroyWindow(login);
    DestroyWindow(cover);
    DestroyWindow(other);
    UnhookWindowsHookEx(hook);
    puts("zero main-window show transitions; resized/repeated show blocked; login, cover and other threads preserved");
    return 0;
}
