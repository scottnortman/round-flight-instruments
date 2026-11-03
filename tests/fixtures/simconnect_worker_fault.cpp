// DEVELOPMENT TEST ONLY. Never distribute this DLL as the Microsoft runtime.
// Exercise the real shim/worker/supervisor with faults inside a loaded DLL.
#include <windows.h>
#include <SimConnect.h>
#include <cstring>
#include <deque>
#include <vector>
#include <fstream>
#include <cstdlib>
static std::deque<std::vector<char>> packets;
static std::vector<char> current;
static int generation = 0, reads = 0;
static bool malformed = false;
template<class T> void enqueue(T value, DWORD id) {
    value.dwSize = sizeof(T); value.dwVersion = 1; value.dwID = id;
    const char* bytes = reinterpret_cast<const char*>(&value);
    packets.emplace_back(bytes, bytes + sizeof(T));
}
static void crash() {
    SetErrorMode(SEM_NOGPFAULTERRORBOX | SEM_FAILCRITICALERRORS);
    RaiseException(EXCEPTION_ACCESS_VIOLATION, EXCEPTION_NONCONTINUABLE, 0, nullptr);
    ExitProcess(99); // Unreachable unless a debugger consumes the exception.
}
#pragma comment(linker, "/EXPORT:SimConnect_Open")
#pragma comment(linker, "/EXPORT:SimConnect_Close")
#pragma comment(linker, "/EXPORT:SimConnect_AddToDataDefinition")
#pragma comment(linker, "/EXPORT:SimConnect_SubscribeToSystemEvent")
#pragma comment(linker, "/EXPORT:SimConnect_RequestSystemState")
#pragma comment(linker, "/EXPORT:SimConnect_RequestDataOnSimObject")
#pragma comment(linker, "/EXPORT:SimConnect_GetNextDispatch")
#define API SIMCONNECTAPI
API SimConnect_Open(HANDLE* handle, LPCSTR, HWND, DWORD, HANDLE, DWORD) {
    auto path = std::getenv("RFI_FIXTURE_STATE");
    if (!path) return E_INVALIDARG; // Cannot accidentally run as a normal source.
    std::ifstream(path) >> generation;
    std::ofstream(path) << ++generation;
    malformed = std::getenv("RFI_FIXTURE_MALFORMED") != nullptr;
    if (!malformed && generation == 2) crash(); // Also exercise failed reopen.
    *handle = reinterpret_cast<HANDLE>(1);
    enqueue(SIMCONNECT_RECV_OPEN{}, SIMCONNECT_RECV_ID_OPEN);
    return S_OK;
}
API SimConnect_Close(HANDLE) { return S_OK; }
API SimConnect_AddToDataDefinition(HANDLE, SIMCONNECT_DATA_DEFINITION_ID, const char*, const char*, SIMCONNECT_DATATYPE, float, DWORD) { return S_OK; }
API SimConnect_SubscribeToSystemEvent(HANDLE, SIMCONNECT_CLIENT_EVENT_ID id, const char*) {
    if (id <= 2) {
        SIMCONNECT_RECV_EVENT e{}; e.uEventID = id; e.dwData = id == 1 ? 1 : 0;
        enqueue(e, SIMCONNECT_RECV_ID_EVENT);
    }
    return S_OK;
}
API SimConnect_RequestSystemState(HANDLE, SIMCONNECT_DATA_REQUEST_ID id, const char*) {
    SIMCONNECT_RECV_SYSTEM_STATE s{}; s.dwRequestID = id; s.dwInteger = 1;
    enqueue(s, SIMCONNECT_RECV_ID_SYSTEM_STATE); return S_OK;
}
API SimConnect_RequestDataOnSimObject(HANDLE, SIMCONNECT_DATA_REQUEST_ID id, SIMCONNECT_DATA_DEFINITION_ID, SIMCONNECT_OBJECT_ID, SIMCONNECT_PERIOD, SIMCONNECT_DATA_REQUEST_FLAG, DWORD, DWORD, DWORD) {
    if (!malformed && generation == 1 && ++reads == 20) crash();
    std::vector<char> bytes(168);
    auto p = reinterpret_cast<SIMCONNECT_RECV_SIMOBJECT_DATA*>(bytes.data());
    p->dwSize = 168; p->dwVersion = 1; p->dwID = SIMCONNECT_RECV_ID_SIMOBJECT_DATA;
    p->dwRequestID = id; p->dwObjectID = generation; p->dwDefineID = 2;
    p->dwDefineCount = malformed ? 15 : 16; p->dwentrynumber = 1; p->dwoutof = 1;
    const double values[] = {generation == 1 ? 4520. : 12000., 4500., generation == 1 ? 29.92 : 29.80, 0., -5., -20., 0., 120., 0., 270., 0., 500., 0., 3., 0., 0.};
    std::memcpy(bytes.data() + 40, values, sizeof(values));
    packets.push_back(bytes); return S_OK;
}
API SimConnect_GetNextDispatch(HANDLE, SIMCONNECT_RECV** data, DWORD* bytes) {
    if (packets.empty()) return E_FAIL;
    current = std::move(packets.front()); packets.pop_front();
    *data = reinterpret_cast<SIMCONNECT_RECV*>(current.data()); *bytes = static_cast<DWORD>(current.size());
    return S_OK;
}
