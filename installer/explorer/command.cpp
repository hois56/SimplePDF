#define UNICODE
#define _UNICODE
#include <windows.h>
#include <shobjidl.h>
#include <shlwapi.h>
#include <strsafe.h>

// This class is stateless. The COM factory, command and server locks share
// the module lifetime counter; Explorer may query them from different threads.
static const CLSID commandId = {0x60715155, 0x560c, 0x4d46, {0xbe, 0xd7, 0x68, 0x4e, 0x5e, 0x4a, 0x5a, 0x96}};
static LONG references;
static HMODULE module;

static HRESULT appPath(wchar_t (&path)[32768]) {
    DWORD length = GetModuleFileNameW(module, path, ARRAYSIZE(path));
    if (length == 0 || length >= ARRAYSIZE(path)) return HRESULT_FROM_WIN32(GetLastError());
    wchar_t* name = wcsrchr(path, L'\\');
    if (!name) return E_UNEXPECTED;
    return StringCchCopyW(name + 1, ARRAYSIZE(path) - (name + 1 - path), L"SimplePDF.exe");
}

static bool singleItem(IShellItemArray* items) {
    DWORD count = 0;
    return items && SUCCEEDED(items->GetCount(&count)) && count == 1;
}

class ConvertCommand final : public IExplorerCommand {
public:
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void** out) override {
        if (!out) return E_POINTER;
        *out = nullptr;
        if (iid != IID_IUnknown && iid != IID_IExplorerCommand) return E_NOINTERFACE;
        *out = static_cast<IExplorerCommand*>(this);
        AddRef();
        return S_OK;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return InterlockedIncrement(&references); }
    ULONG STDMETHODCALLTYPE Release() override { return InterlockedDecrement(&references); }
    HRESULT STDMETHODCALLTYPE GetTitle(IShellItemArray*, LPWSTR* title) override {
        return SHStrDupW(L"PDF로 변환", title);
    }
    HRESULT STDMETHODCALLTYPE GetIcon(IShellItemArray*, LPWSTR* icon) override {
        wchar_t path[32768];
        HRESULT hr = appPath(path);
        return FAILED(hr) ? hr : SHStrDupW(path, icon);
    }
    HRESULT STDMETHODCALLTYPE GetToolTip(IShellItemArray*, LPWSTR* tip) override {
        *tip = nullptr;
        return E_NOTIMPL;
    }
    HRESULT STDMETHODCALLTYPE GetCanonicalName(GUID* id) override { *id = commandId; return S_OK; }
    HRESULT STDMETHODCALLTYPE GetState(IShellItemArray* items, BOOL, EXPCMDSTATE* state) override {
        *state = singleItem(items) ? ECS_ENABLED : ECS_HIDDEN;
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE Invoke(IShellItemArray* items, IBindCtx*) override {
        if (!singleItem(items)) return E_INVALIDARG;
        IShellItem* item = nullptr;
        HRESULT hr = items->GetItemAt(0, &item);
        if (FAILED(hr)) return hr;
        PWSTR source = nullptr;
        hr = item->GetDisplayName(SIGDN_FILESYSPATH, &source);
        item->Release();
        if (FAILED(hr)) return hr;

        wchar_t path[32768], args[32768];
        hr = appPath(path);
        if (SUCCEEDED(hr)) hr = StringCchPrintfW(args, ARRAYSIZE(args), L"\"%ls\" --to-pdf \"%ls\"", path, source);
        CoTaskMemFree(source);
        if (FAILED(hr)) return hr;
        STARTUPINFOW startup{};
        startup.cb = sizeof(startup);
        PROCESS_INFORMATION child{};
        if (!CreateProcessW(path, args, nullptr, nullptr, FALSE, 0, nullptr, nullptr, &startup, &child))
            return HRESULT_FROM_WIN32(GetLastError());
        CloseHandle(child.hThread);
        CloseHandle(child.hProcess);
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE GetFlags(EXPCMDFLAGS* flags) override { *flags = ECF_DEFAULT; return S_OK; }
    HRESULT STDMETHODCALLTYPE EnumSubCommands(IEnumExplorerCommand** commands) override {
        *commands = nullptr;
        return E_NOTIMPL;
    }
};

static ConvertCommand command;
class Factory final : public IClassFactory {
public:
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void** out) override {
        if (!out) return E_POINTER;
        *out = nullptr;
        if (iid != IID_IUnknown && iid != IID_IClassFactory) return E_NOINTERFACE;
        *out = static_cast<IClassFactory*>(this);
        AddRef();
        return S_OK;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return InterlockedIncrement(&references); }
    ULONG STDMETHODCALLTYPE Release() override { return InterlockedDecrement(&references); }
    HRESULT STDMETHODCALLTYPE CreateInstance(IUnknown* outer, REFIID iid, void** out) override {
        if (!out) return E_POINTER;
        *out = nullptr;
        return outer ? CLASS_E_NOAGGREGATION : command.QueryInterface(iid, out);
    }
    HRESULT STDMETHODCALLTYPE LockServer(BOOL lock) override {
        if (lock) AddRef(); else Release();
        return S_OK;
    }
};

static Factory factory;
extern "C" __declspec(dllexport) HRESULT WINAPI DllGetClassObject(REFCLSID id, REFIID iid, void** out) {
    if (!out) return E_POINTER;
    *out = nullptr;
    return id == commandId ? factory.QueryInterface(iid, out) : CLASS_E_CLASSNOTAVAILABLE;
}
extern "C" __declspec(dllexport) HRESULT WINAPI DllCanUnloadNow() {
    return InterlockedCompareExchange(&references, 0, 0) == 0 ? S_OK : S_FALSE;
}
BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID) {
    if (reason == DLL_PROCESS_ATTACH) module = instance;
    return TRUE;
}
