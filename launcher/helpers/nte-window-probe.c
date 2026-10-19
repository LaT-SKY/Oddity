#define UNICODE
#define _UNICODE
#include <windows.h>
#include <tlhelp32.h>
#include <stdio.h>
#include <stdarg.h>
#include <stdlib.h>
#include <wchar.h>

/*
 * Read-only window topology probe for NTEGame.exe.
 * Logs top-level windows belonging to the launcher and their child windows,
 * with class, title, rect, style, owner and parent. Only changes are logged.
 *
 * Env:
 *   NTE_WINDOW_OUT          Windows log path (optional; stderr otherwise)
 *   NTE_WINDOW_PID          target pid (optional; otherwise find NTEGame.exe)
 *   NTE_WINDOW_DURATION_MS  default 30000
 *   NTE_WINDOW_INTERVAL_MS  default 150
 */

static FILE *g_log;
static DWORD g_start;
static DWORD g_pid;

static void logf_(const char *fmt, ...)
{
    va_list ap;
    char line[4096];
    va_start(ap, fmt);
    _vsnprintf(line, sizeof(line) - 2, fmt, ap);
    va_end(ap);
    line[sizeof(line) - 2] = 0;
    strcat(line, "\n");
    if (g_log) { fputs(line, g_log); fflush(g_log); }
    fputs(line, stderr);
}

static DWORD find_pid(void)
{
    HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    PROCESSENTRY32W pe;
    DWORD found = 0;
    if (snap == INVALID_HANDLE_VALUE) return 0;
    pe.dwSize = sizeof(pe);
    if (Process32FirstW(snap, &pe)) {
        do {
            if (wcsstr(pe.szExeFile, L"NTEGame.exe")) { found = pe.th32ProcessID; break; }
        } while (Process32NextW(snap, &pe));
    }
    CloseHandle(snap);
    return found;
}

static void append_window(char *buf, size_t cap, HWND hwnd, int depth)
{
    wchar_t cls[256] = L"";
    wchar_t title[512] = L"";
    RECT r = {0,0,0,0};
    DWORD pid = 0;
    LONG style, exstyle;
    HWND owner, parent;
    int n;

    GetClassNameW(hwnd, cls, 255);
    GetWindowTextW(hwnd, title, 511);
    GetWindowRect(hwnd, &r);
    GetWindowThreadProcessId(hwnd, &pid);
    style = (LONG)GetWindowLongPtrW(hwnd, GWL_STYLE);
    exstyle = (LONG)GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    owner = GetWindow(hwnd, GW_OWNER);
    parent = GetParent(hwnd);

    n = _snprintf(buf + strlen(buf), cap - strlen(buf) - 1,
        "%*sHWND=%p vis=%d pid=%lu cls=%ls title=%ls rect=%ld,%ld,%ld,%ld style=%08lx ex=%08lx owner=%p parent=%p\n",
        depth * 2, "", (void *)hwnd, IsWindowVisible(hwnd) ? 1 : 0,
        (unsigned long)pid, cls, title,
        (long)r.left, (long)r.top, (long)r.right, (long)r.bottom,
        (unsigned long)style, (unsigned long)exstyle,
        (void *)owner, (void *)parent);
    (void)n;
}

typedef struct { HWND hwnd; int depth; } WINITEM;

static BOOL CALLBACK enum_child(HWND hwnd, LPARAM lp)
{
    char *buf = (char *)lp;
    size_t cap = 65536;
    append_window(buf, cap, hwnd, 1);
    return TRUE;
}

static BOOL CALLBACK enum_top(HWND hwnd, LPARAM lp)
{
    DWORD pid = 0;
    char *buf = (char *)lp;
    GetWindowThreadProcessId(hwnd, &pid);
    if (pid != g_pid) return TRUE;
    append_window(buf, 65536, hwnd, 0);
    EnumChildWindows(hwnd, enum_child, lp);
    return TRUE;
}

int WINAPI WinMain(HINSTANCE inst, HINSTANCE prev, LPSTR cmdline, int show)
{
    wchar_t out[1024];
    DWORD pid = 0, duration = 30000, interval = 150;
    char prevbuf[65536] = "";
    DWORD last = 0;
    (void)inst; (void)prev; (void)cmdline; (void)show;

    g_start = GetTickCount();
    if (GetEnvironmentVariableW(L"NTE_WINDOW_OUT", out, 1024) && out[0])
        g_log = _wfopen(out, L"wb");
    if (!g_log) g_log = stderr;

    pid = GetEnvironmentVariableW(L"NTE_WINDOW_PID", out, 32) ? (DWORD)wcstoul(out, NULL, 10) : 0;
    if (!pid) pid = find_pid();
    if (!pid) { logf_("ERR NTEGame.exe not found"); return 2; }
    logf_("target pid=%lu", (unsigned long)pid);

    {
        wchar_t d[32];
        if (GetEnvironmentVariableW(L"NTE_WINDOW_DURATION_MS", d, 32)) duration = (DWORD)wcstoul(d, NULL, 10);
        if (GetEnvironmentVariableW(L"NTE_WINDOW_INTERVAL_MS", d, 32)) interval = (DWORD)wcstoul(d, NULL, 10);
    }
    if (interval < 30) interval = 30;
    logf_("duration=%lu interval=%lu", (unsigned long)duration, (unsigned long)interval);

    g_pid = pid;

    while ((GetTickCount() - g_start) < duration) {
        char buf[65536];
        buf[0] = 0;
        EnumWindows(enum_top, (LPARAM)buf);
        if (strcmp(buf, prevbuf) != 0) {
            logf_("=== t=%lu ===", (unsigned long)(GetTickCount() - g_start));
            fputs(buf, g_log); fflush(g_log); fputs(buf, stderr);
            strncpy(prevbuf, buf, sizeof(prevbuf) - 1);
            prevbuf[sizeof(prevbuf) - 1] = 0;
            last = GetTickCount();
        }
        Sleep(interval);
    }
    (void)last;
    if (g_log && g_log != stderr) fclose(g_log);
    return 0;
}
