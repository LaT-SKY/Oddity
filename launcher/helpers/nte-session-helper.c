#define UNICODE
#define _UNICODE
#include <windows.h>
#include <tlhelp32.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>
#include <io.h>
#include <fcntl.h>

/* 官方后台负责游戏创建及登录, 此进程只控制本轮启动时序和生命周期. */
static FILE *session_output;
static FILE *status_stream(void) { return session_output ? session_output : stderr; }

static int fail(const char *text)
{
    fprintf(status_stream(), "ERR %s (winerr=%lu)\n", text, (unsigned long)GetLastError());
    return 1;
}

static int native_name(const wchar_t *name)
{
    return !_wcsicmp(name, L"NTEGame.exe") || !_wcsicmp(name, L"NTEGlobalGame.exe");
}

static int verify_handle(HANDLE h, wchar_t *path, DWORD capacity)
{
    DWORD size = capacity;
    wchar_t *name;
    BOOL wow64 = FALSE;
    if (!QueryFullProcessImageNameW(h, 0, path, &size) || !size || size >= capacity) return fail("query process image failed");
    path[size] = 0;
    name = wcsrchr(path, L'\\');
    if (!name) name = wcsrchr(path, L'/');
    name = name ? name + 1 : path;
    if (!native_name(name)) return fail("target image is not an NTE native client");
    if (!IsWow64Process(h, &wow64) || wow64) return fail("target is not native x64");
    return 0;
}

/* 路径仅由已核验的官方后台位置派生, 不接受外部可执行文件参数. */
static int game_paths(const wchar_t *image, wchar_t *exe, wchar_t *cwd, SIZE_T capacity)
{
    static const wchar_t suffix[] = L"\\Client\\WindowsNoEditor\\HT\\Binaries\\Win64\\HTGame.exe";
    wchar_t *name, *parent, *last;
    SIZE_T len = wcslen(image), i;
    if (len >= capacity) return 0;
    wmemcpy(cwd, image, len + 1);
    for (i = 0; i < len; ++i) if (cwd[i] == L'/') cwd[i] = L'\\';
    name = wcsrchr(cwd, L'\\');
    if (!name || !native_name(name + 1)) return 0;
    int global = !_wcsicmp(name + 1, L"NTEGlobalGame.exe");
    *name = 0;
    parent = wcsrchr(cwd, L'\\');
    if (!parent || _wcsicmp(parent + 1, global ? L"NTEGlobal" : L"NTELauncher")) return 0;
    *parent = 0;
    len = wcslen(cwd);
    if (!len || len + wcslen(suffix) >= capacity) return 0;
    wmemcpy(exe, cwd, len);
    wcscpy(exe + len, suffix);
    wcscpy(cwd, exe);
    last = wcsrchr(cwd, L'\\');
    if (!last) return 0;
    *last = 0;
    return 1;
}

static int absolute_windows_path(const wchar_t *path)
{
    return (path[0] && path[1] == L':' && (path[2] == L'\\' || path[2] == L'/')) ||
           (path[0] == L'\\' && path[1] == L'\\');
}

static int session_setting(const wchar_t *key, wchar_t *out, DWORD capacity)
{
    DWORD size = GetEnvironmentVariableW(key, out, capacity);
    if (!size || size >= capacity || !absolute_windows_path(out)) return fail("invalid session path setting");
    return 0;
}

/* 区分空快照与查询失败, 清理完成必须有可信的末次检查. */
static int session_native_pid(DWORD helper_pid, DWORD *pid)
{
    HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    PROCESSENTRY32W entry;
    DWORD found = 0, error;
    unsigned int matches = 0;
    if (snapshot == INVALID_HANDLE_VALUE) return -1;
    ZeroMemory(&entry, sizeof(entry));
    entry.dwSize = sizeof(entry);
    if (Process32FirstW(snapshot, &entry)) {
        do {
            if (entry.th32ParentProcessID == helper_pid && native_name(entry.szExeFile)) {
                found = entry.th32ProcessID;
                ++matches;
            }
        } while (Process32NextW(snapshot, &entry));
        error = GetLastError();
    } else error = GetLastError();
    CloseHandle(snapshot);
    if (error != ERROR_NO_MORE_FILES || matches > 1) return -1;
    *pid = found;
    return matches ? 1 : 0;
}

static int capture_session_native(DWORD helper_pid, HANDLE *native, DWORD *native_pid,
                                  wchar_t *image, DWORD capacity, int cleanup)
{
    DWORD found = 0;
    HANDLE handle;
    int scanned = session_native_pid(helper_pid, &found);
    if (scanned < 0) return fail("scan this session native failed or ambiguous");
    if (!scanned) return 0;
    if (*native) {
        if (found != *native_pid) return fail("additional native from this session helper rejected");
        return 0;
    }
    handle = OpenProcess(PROCESS_QUERY_INFORMATION | SYNCHRONIZE | PROCESS_TERMINATE, FALSE, found);
    if (!handle) {
        /* 快照与打开之间允许进程自行退出, 其它失败留待清理重试. */
        if (GetLastError() == ERROR_INVALID_PARAMETER) return 0;
        return fail("open this session native failed");
    }
    if (verify_handle(handle, image, capacity)) { CloseHandle(handle); return 1; }
    *native = handle;
    *native_pid = found;
    fprintf(status_stream(), "NATIVE_STARTED windows_pid=%lu parent_windows_pid=%lu%s\n",
            (unsigned long)found, (unsigned long)helper_pid, cleanup ? " phase=cleanup" : "");
    fflush(status_stream());
    return 0;
}

/* 仅确认句柄已触发且退出码有效后才报告退出. */
static int confirmed_exit(HANDLE process, DWORD *code)
{
    return WaitForSingleObject(process, 0) == WAIT_OBJECT_0 &&
           GetExitCodeProcess(process, code) && *code != STILL_ACTIVE;
}

static int terminate_and_confirm(HANDLE process)
{
    DWORD code = STILL_ACTIVE;
    if (confirmed_exit(process, &code)) return 1;
    if (!TerminateProcess(process, 125)) fail("terminate this session process failed");
    if (WaitForSingleObject(process, 5000) != WAIT_OBJECT_0 || !confirmed_exit(process, &code)) {
        fail("this session process exit remains unconfirmed");
        return 0;
    }
    return 1;
}

static BOOL CALLBACK close_native_window(HWND window, LPARAM parameter)
{
    DWORD pid = 0;
    GetWindowThreadProcessId(window, &pid);
    if (pid == (DWORD)(ULONG_PTR)parameter) PostMessageW(window, WM_CLOSE, 0, 0);
    return TRUE;
}

static int setup_session_output(void)
{
    wchar_t path[32768];
    DWORD size = GetEnvironmentVariableW(L"NTE_SESSION_OUTPUT", path, 32768);
    HANDLE handle;
    int fd;
    if (!size) return 0;
    if (size >= 32768 || !absolute_windows_path(path)) return fail("invalid session output path");
    handle = CreateFileW(path, GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL,
                         CREATE_NEW, FILE_ATTRIBUTE_NORMAL, NULL);
    if (handle == INVALID_HANDLE_VALUE) return fail("open dedicated session output failed");
    fd = _open_osfhandle((intptr_t)handle, _O_WRONLY | _O_BINARY);
    if (fd < 0) { CloseHandle(handle); return fail("open session output descriptor failed"); }
    session_output = _fdopen(fd, "wb");
    if (!session_output) { _close(fd); return fail("open session output stream failed"); }
    setvbuf(session_output, NULL, _IONBF, 0);
    return 0;
}

typedef struct {
    DWORD pid;
    int width, height, x, y;
    HWND window;
    unsigned int matches;
} CLICK_TARGET;

static int parse_click(const wchar_t *text, CLICK_TARGET *target)
{
    unsigned int values[4];
    int i;
    for (i = 0; i < 4; ++i) {
        unsigned int value = 0;
        if (*text < L'0' || *text > L'9') return 0;
        do {
            value = value * 10 + (unsigned int)(*text++ - L'0');
            if (value > 32767) return 0;
        } while (*text >= L'0' && *text <= L'9');
        values[i] = value;
        if (i < 3) { if (*text++ != L',') return 0; }
        else if (*text) return 0;
    }
    if (!values[0] || !values[1] || values[2] >= values[0] || values[3] >= values[1]) return 0;
    target->width = (int)values[0]; target->height = (int)values[1];
    target->x = (int)values[2]; target->y = (int)values[3];
    return 1;
}

static BOOL CALLBACK find_click_window(HWND window, LPARAM parameter)
{
    CLICK_TARGET *target = (CLICK_TARGET *)parameter;
    wchar_t cls[256];
    DWORD pid = 0;
    RECT client;
    GetWindowThreadProcessId(window, &pid);
    if (pid != target->pid || !GetClassNameW(window, cls, 256) || !wcsstr(cls, L"QWindow") ||
        !GetClientRect(window, &client) || client.right - client.left != target->width ||
        client.bottom - client.top != target->height) return TRUE;
    target->window = window;
    ++target->matches;
    return TRUE;
}

static int click_official_start(CLICK_TARGET *target)
{
    const UINT messages[] = {WM_MOUSEMOVE, WM_LBUTTONDOWN, WM_LBUTTONUP};
    int i;
    target->matches = 0;
    target->window = NULL;
    EnumWindows(find_click_window, (LPARAM)target);
    if (target->matches != 1) return fail("need exactly one matching native QWindow client size");
    for (i = 0; i < 3; ++i) {
        DWORD_PTR ignored = 0;
        DWORD pid = 0;
        RECT client;
        GetWindowThreadProcessId(target->window, &pid);
        if (pid != target->pid || !GetClientRect(target->window, &client) ||
            client.right - client.left != target->width || client.bottom - client.top != target->height)
            return fail("native click target changed");
        if (!SendMessageTimeoutW(target->window, messages[i], i == 1 ? MK_LBUTTON : 0,
                                 MAKELPARAM(target->x, target->y), SMTO_ABORTIFHUNG | SMTO_BLOCK,
                                 2000, &ignored)) return fail("native button message timed out or failed");
    }
    fputs("OFFICIAL_START_CLICK_SENT\n", status_stream());
    fflush(status_stream());
    return 0;
}

static int native_game_pid(DWORD native_pid, DWORD *pid)
{
    HANDLE snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    PROCESSENTRY32W entry;
    DWORD found = 0, error;
    unsigned int matches = 0;
    if (snapshot == INVALID_HANDLE_VALUE) return -1;
    ZeroMemory(&entry, sizeof(entry));
    entry.dwSize = sizeof(entry);
    if (Process32FirstW(snapshot, &entry)) {
        do {
            if (entry.th32ParentProcessID == native_pid && !_wcsicmp(entry.szExeFile, L"HTGame.exe")) {
                found = entry.th32ProcessID;
                ++matches;
            }
        } while (Process32NextW(snapshot, &entry));
        error = GetLastError();
    } else error = GetLastError();
    CloseHandle(snapshot);
    if (error != ERROR_NO_MORE_FILES || matches > 1) return -1;
    *pid = found;
    return matches ? 1 : 0;
}

static int verify_game(HANDLE game, const wchar_t *native_image)
{
    wchar_t actual[32768], expected[32768], cwd[32768], *name;
    DWORD size = 32768;
    BY_HANDLE_FILE_INFORMATION actual_info, expected_info;
    HANDLE actual_file, expected_file;
    int same;
    if (!QueryFullProcessImageNameW(game, 0, actual, &size) || !size || size >= 32768) return fail("query official game image failed");
    actual[size] = 0;
    name = wcsrchr(actual, L'\\');
    if (!name) name = wcsrchr(actual, L'/');
    if (!name || _wcsicmp(name + 1, L"HTGame.exe") || !game_paths(native_image, expected, cwd, 32768)) return fail("official game image rejected");
    actual_file = CreateFileW(actual, 0, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, NULL, OPEN_EXISTING, 0, NULL);
    if (actual_file == INVALID_HANDLE_VALUE) return fail("open official game image identity failed");
    expected_file = CreateFileW(expected, 0, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, NULL, OPEN_EXISTING, 0, NULL);
    if (expected_file == INVALID_HANDLE_VALUE) { CloseHandle(actual_file); return fail("open fixed installed game identity failed"); }
    same = GetFileInformationByHandle(actual_file, &actual_info) && GetFileInformationByHandle(expected_file, &expected_info) &&
           (actual_info.nFileIndexHigh || actual_info.nFileIndexLow) &&
           actual_info.dwVolumeSerialNumber == expected_info.dwVolumeSerialNumber &&
           actual_info.nFileIndexHigh == expected_info.nFileIndexHigh && actual_info.nFileIndexLow == expected_info.nFileIndexLow;
    CloseHandle(actual_file);
    CloseHandle(expected_file);
    if (!same) return fail("official game differs from fixed installed HTGame.exe");
    return 0;
}

static int observe_official_game(HANDLE native, DWORD native_pid, const wchar_t *native_image,
                                 const wchar_t *cancel, int *game_confirmed)
{
    ULONGLONG started = GetTickCount64(), stop_started = 0, close_started = 0;
    HANDLE game = NULL;
    DWORD pid = 0, code = 1, native_code = STILL_ACTIVE;
    int result, scanned, stop_requested = 0, force_failed = 0;
    *game_confirmed = 0;
    for (;;) {
        scanned = native_game_pid(native_pid, &pid);
        if (scanned < 0) return fail("official game snapshot failed or ambiguous");
        if (scanned) {
            game = OpenProcess(PROCESS_QUERY_INFORMATION | SYNCHRONIZE | PROCESS_TERMINATE, FALSE, pid);
            if (!game) game = OpenProcess(PROCESS_QUERY_INFORMATION | SYNCHRONIZE, FALSE, pid);
            if (!game) {
                fail("official game handle unavailable; waiting for child disappearance");
                for (;;) {
                    DWORD remaining = 0;
                    scanned = native_game_pid(native_pid, &remaining);
                    if (scanned < 0) return fail("official game disappearance could not be confirmed");
                    if (!scanned) break;
                    if (remaining != pid) return fail("official game changed during disappearance wait");
                    Sleep(100);
                }
                *game_confirmed = 1;
                fputs("GAME_EXITED code=unavailable\n", status_stream());
                return 4;
            }
            break;
        }
        if (confirmed_exit(native, &native_code)) {
            /* 后台停止后再次扫描, 不把创建与退出之间的空快照当成无游戏. */
            scanned = native_game_pid(native_pid, &pid);
            if (scanned < 0) return fail("final official game snapshot failed or ambiguous");
            if (scanned) continue;
            *game_confirmed = 1;
            return fail(stop_requested ? "session stopped before official game started" : "official native exited before game started");
        }
        if (!stop_requested && (GetFileAttributesW(cancel) != INVALID_FILE_ATTRIBUTES ||
                                GetTickCount64() - started >= 90000)) {
            stop_requested = 1;
            stop_started = GetTickCount64();
            fail("stopping official launch source before final game check");
        }
        if (stop_requested) {
            EnumWindows(close_native_window, (LPARAM)(ULONG_PTR)native_pid);
            if (GetTickCount64() - stop_started >= 8000 && !terminate_and_confirm(native))
                return fail("official launch source exit not confirmed");
        }
        Sleep(100);
    }
    result = verify_game(game, native_image);
    if (!result) {
        fprintf(status_stream(), "GAME_STARTED windows_pid=%lu parent_windows_pid=%lu\n", (unsigned long)pid, (unsigned long)native_pid);
        fflush(status_stream());
    } else fputs("GAME_OBSERVED_BUT_REJECTED waiting_for_exit\n", status_stream());
    for (;;) {
        DWORD wait = WaitForSingleObject(game, 100);
        if (wait == WAIT_OBJECT_0) break;
        if (wait != WAIT_TIMEOUT) { result = fail("wait for official game failed"); break; }
        if (GetFileAttributesW(cancel) != INVALID_FILE_ATTRIBUTES && !result && !force_failed) {
            if (!close_started) {
                close_started = GetTickCount64();
                EnumWindows(close_native_window, (LPARAM)(ULONG_PTR)pid);
            } else if (GetTickCount64() - close_started > 8000 && !terminate_and_confirm(game)) {
                /* 终止被拒绝时继续持有游戏及后台, 等待游戏自行退出. */
                force_failed = 1;
                result = fail("official game close unconfirmed; waiting for game exit");
            }
        }
    }
    if (!confirmed_exit(game, &code)) result = fail("official game exit not confirmed");
    else {
        *game_confirmed = 1;
        fprintf(status_stream(), "GAME_EXITED code=%lu\n", (unsigned long)code);
    }
    fflush(status_stream());
    CloseHandle(game);
    return result ? result : (int)code;
}

static int run_session(void)
{
    wchar_t helper[32768], signal[32768], cancel[32768], image[32768], click_text[128];
    wchar_t command[] = L"nte-launch-helper.exe";
    wchar_t *name;
    CLICK_TARGET click;
    STARTUPINFOW startup;
    PROCESS_INFORMATION process;
    HANDLE native = NULL;
    DWORD native_pid = 0, attributes;
    ULONGLONG started;
    int result = 1, game_confirmed = 1;
    ZeroMemory(&click, sizeof(click));
    if (setup_session_output()) return 6;
    if (session_setting(L"NTE_SESSION_HELPER", helper, 32768) ||
        session_setting(L"NTE_SESSION_SIGNAL", signal, 32768) ||
        session_setting(L"NTE_SESSION_CANCEL", cancel, 32768)) return 6;
    attributes = GetEnvironmentVariableW(L"NTE_SESSION_CLICK", click_text, 128);
    if (!attributes || attributes >= 128 || !parse_click(click_text, &click)) return fail("invalid NTE_SESSION_CLICK width,height,x,y");
    name = wcsrchr(helper, L'\\');
    if (!name) name = wcsrchr(helper, L'/');
    if (!name || _wcsicmp(name + 1, L"nte-launch-helper.exe")) return fail("session helper basename rejected");
    attributes = GetFileAttributesW(helper);
    if (attributes == INVALID_FILE_ATTRIBUTES || (attributes & FILE_ATTRIBUTE_DIRECTORY)) return fail("session helper missing");
    if (GetFileAttributesW(signal) != INVALID_FILE_ATTRIBUTES) return fail("session signal already exists");
    ZeroMemory(&startup, sizeof(startup));
    ZeroMemory(&process, sizeof(process));
    startup.cb = sizeof(startup);
    if (!CreateProcessW(helper, command, NULL, NULL, FALSE, 0, NULL, NULL, &startup, &process)) return fail("CreateProcessW session helper failed");
    CloseHandle(process.hThread);
    fprintf(status_stream(), "HELPER_STARTED windows_pid=%lu\n", (unsigned long)process.dwProcessId);
    fflush(status_stream());
    started = GetTickCount64();
    while (GetTickCount64() - started < 210000) {
        if (WaitForSingleObject(process.hProcess, 0) != WAIT_TIMEOUT) {
            fail("session helper exited before resource signal");
            goto cleanup;
        }
        if (!native && capture_session_native(process.dwProcessId, &native, &native_pid, image, 32768, 0)) goto cleanup;
        if (GetFileAttributesW(cancel) != INVALID_FILE_ATTRIBUTES) {
            fail("session cancelled before resource signal");
            goto cleanup;
        }
        attributes = GetFileAttributesW(signal);
        if (attributes != INVALID_FILE_ATTRIBUTES && !(attributes & FILE_ATTRIBUTE_DIRECTORY)) {
            if (!native) { fail("resource signal arrived without this session native"); goto cleanup; }
            if (WaitForSingleObject(native, 0) != WAIT_TIMEOUT) { fail("native exited before environment handoff"); goto cleanup; }
            click.pid = native_pid;
            result = click_official_start(&click);
            /* 即使按钮消息超时, 也先检查可能已经启动的官方游戏. */
            {
                int observed = observe_official_game(native, native_pid, image, cancel, &game_confirmed);
                if (observed) result = observed;
            }
            goto cleanup;
        }
        Sleep(100);
    }
    fail("timed out waiting 210 seconds for resource signal");
cleanup:
    {
        DWORD native_code = STILL_ACTIVE, helper_code = STILL_ACTIVE;
        int complete = game_confirmed, helper_done;
        /* helper 仍可能创建子进程, 宽限期内持续发现并关闭本轮 native. */
        started = GetTickCount64();
        do {
            (void)capture_session_native(process.dwProcessId, &native, &native_pid, image, 32768, 1);
            if (native && WaitForSingleObject(native, 0) == WAIT_TIMEOUT)
                EnumWindows(close_native_window, (LPARAM)(ULONG_PTR)native_pid);
            if (confirmed_exit(process.hProcess, &helper_code)) break;
            Sleep(100);
        } while (GetTickCount64() - started < 10000);
        /* 先阻止 helper 再创建进程, 再检查宽限期尾部创建的 native. */
        helper_done = terminate_and_confirm(process.hProcess);
        if (!helper_done) complete = 0;
        if (capture_session_native(process.dwProcessId, &native, &native_pid, image, 32768, 1)) complete = 0;
        if (native) {
            if (WaitForSingleObject(native, 0) == WAIT_TIMEOUT) {
                EnumWindows(close_native_window, (LPARAM)(ULONG_PTR)native_pid);
                (void)WaitForSingleObject(native, 2000);
            }
            if (!terminate_and_confirm(native) || !confirmed_exit(native, &native_code)) complete = 0;
            else fprintf(status_stream(), "NATIVE_EXITED code=%lu\n", (unsigned long)native_code);
            CloseHandle(native);
        }
        if (helper_done && confirmed_exit(process.hProcess, &helper_code))
            fprintf(status_stream(), "HELPER_EXITED code=%lu\n", (unsigned long)helper_code);
        else complete = 0;
        CloseHandle(process.hProcess);
        if (complete) fprintf(status_stream(), "SESSION_EXITED code=%lu\n", (unsigned long)(DWORD)result);
        else {
            fputs("SESSION_CLEANUP_INCOMPLETE\n", status_stream());
            result = 1;
        }
        fflush(status_stream());
    }
    return result;
}

int WINAPI WinMain(HINSTANCE instance, HINSTANCE previous, LPSTR arguments, int show)
{
    int result;
    (void)instance; (void)previous; (void)arguments; (void)show;
    result = run_session();
    if (session_output && fclose(session_output) != 0) return 7;
    return result;
}
