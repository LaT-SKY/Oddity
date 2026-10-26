#define UNICODE
#define _UNICODE
#include <windows.h>
#include <commctrl.h>
#include <wchar.h>

static int profile_width, profile_height, profile_loaded;
static const UINT_PTR subclass_id = 0x4e5445;

static int read_profile(void)
{
    wchar_t text[128], extra;
    unsigned int width, height, x, y;
    DWORD size;
    if (profile_loaded) return profile_loaded > 0;
    profile_loaded = -1;
    size = GetEnvironmentVariableW(L"NTE_SESSION_CLICK", text, 128);
    if (!size || size >= 128 || swscanf(text, L"%u,%u,%u,%u%lc", &width, &height, &x, &y, &extra) != 4 ||
        !width || !height || width > 32767 || height > 32767 || x >= width || y >= height) return 0;
    profile_width = (int)width; profile_height = (int)height; profile_loaded = 1;
    return 1;
}

static int is_candidate(HWND window)
{
    wchar_t cls[256];
    DWORD pid = 0;
    if (!GetWindowThreadProcessId(window, &pid)) return 0;
    if (pid != GetCurrentProcessId() || GetAncestor(window, GA_ROOT) != window ||
        (GetWindowLongPtrW(window, GWL_STYLE) & WS_CHILD)) return 0;
    if (!GetClassNameW(window, cls, 256)) return 0;
    return wcsstr(cls, L"QWindow") && !wcsstr(cls, L"ToolSaveBitsOwnDC");
}

/* 使用即将应用的客户区尺寸识别主窗口, 不等待窗口先显示. */
static int main_size(HWND window, const WINDOWPOS *position)
{
    RECT client, outer;
    if (!GetClientRect(window, &client) || !GetWindowRect(window, &outer)) return 0;
    if (position->flags & SWP_NOSIZE)
        return client.right == profile_width && client.bottom == profile_height;
    return position->cx - (outer.right - outer.left - client.right) == profile_width &&
           position->cy - (outer.bottom - outer.top - client.bottom) == profile_height;
}

/* 在窗口位置请求生效前移除显示标志, 避免先映射再隐藏. */
static LRESULT CALLBACK guard_window(HWND window, UINT message, WPARAM wparam, LPARAM lparam,
                                     UINT_PTR id, DWORD_PTR identified)
{
    LRESULT result;
    if (message == WM_NCDESTROY) RemoveWindowSubclass(window, guard_window, id);
    result = DefSubclassProc(window, message, wparam, lparam);
    if (message == WM_WINDOWPOSCHANGING && is_candidate(window)) {
        WINDOWPOS *position = (WINDOWPOS *)lparam;
        if (identified || main_size(window, position)) {
            if (!identified) SetWindowSubclass(window, guard_window, id, 1);
            position->flags &= ~SWP_SHOWWINDOW;
            position->flags |= SWP_HIDEWINDOW;
        }
    }
    return result;
}

__declspec(dllexport) BOOL WINAPI OddityWindowGuardReady(void)
{
    return read_profile();
}

/* hook 仅注册到本轮官方组件的主线程, 不传递给游戏进程. */
__declspec(dllexport) LRESULT CALLBACK OddityWindowHook(int code, WPARAM wparam, LPARAM lparam)
{
    if (code == HCBT_CREATEWND && read_profile() && is_candidate((HWND)wparam))
        if (!SetWindowSubclass((HWND)wparam, guard_window, subclass_id, 0)) return 1;
    return CallNextHookEx(NULL, code, wparam, lparam);
}

BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID reserved)
{
    (void)reserved;
    if (reason == DLL_PROCESS_ATTACH) DisableThreadLibraryCalls(instance);
    return TRUE;
}
