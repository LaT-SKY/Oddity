#define WinMain session_helper_entry
#include "../nte-session-helper.c"
#undef WinMain
#include <assert.h>

int main(void)
{
    wchar_t exe[32768], cwd[32768], image[32768];
    assert(native_name(L"NTEGame.exe"));
    assert(native_name(L"NTEGlobalGame.exe"));
    assert(native_name(L"nteglobalgame.EXE"));
    assert(!native_name(L"OtherGame.exe"));
    assert(game_paths(L"Z:\\games\\NTELauncher\\NTEGame.exe", exe, cwd, 32768));
    assert(!wcscmp(exe, L"Z:\\games\\Client\\WindowsNoEditor\\HT\\Binaries\\Win64\\HTGame.exe"));
    assert(game_paths(L"Z:/games/NTEGlobal/NTEGlobalGame.exe", exe, cwd, 32768));
    assert(!wcscmp(exe, L"Z:\\games\\Client\\WindowsNoEditor\\HT\\Binaries\\Win64\\HTGame.exe"));
    assert(!game_paths(L"Z:\\games\\NTELauncher\\NTEGlobalGame.exe", exe, cwd, 32768));
    assert(!game_paths(L"Z:\\games\\NTEGlobal\\NTEGame.exe", exe, cwd, 32768));
    assert(!game_paths(L"Z:\\games\\NTEGlobal\\OtherGame.exe", exe, cwd, 32768));
    assert(!game_paths(L"Z:\\games\\NTEGlobal\\NTEGlobalGame.exe", exe, cwd, 16));
    assert(!verify_handle(GetCurrentProcess(), image, 32768));
    puts("CN/global helper image and path checks passed");
    return 0;
}
