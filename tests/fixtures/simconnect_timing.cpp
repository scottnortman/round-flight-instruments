// TEST ONLY: delayed, moving SimConnect replies. Never a Microsoft runtime.
// Requires an explicit test state path; production executables are unchanged.
#include <windows.h>
#include <SimConnect.h>
#include <cmath>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <string>
#include <vector>

struct Packet { ULONGLONG due; std::vector<char> bytes; };
static std::vector<Packet> queue;
static std::vector<char> current;
static ULONGLONG started;
static unsigned generation, requests, pulse_bucket;
static bool switched, delayed;
static std::string root, mode;

template<class T> void enqueue(T value, DWORD id) {
    value.dwSize = sizeof(T); value.dwVersion = 1; value.dwID = id;
    const char* p = reinterpret_cast<const char*>(&value);
    queue.push_back({GetTickCount64(), std::vector<char>(p, p + sizeof(T))});
}
static void write_attempt() {
    std::ofstream(root + ".writes", std::ios::app) << "unexpected control\n";
}
#pragma comment(linker, "/EXPORT:SimConnect_Open")
#pragma comment(linker, "/EXPORT:SimConnect_Close")
#pragma comment(linker, "/EXPORT:SimConnect_AddToDataDefinition")
#pragma comment(linker, "/EXPORT:SimConnect_SubscribeToSystemEvent")
#pragma comment(linker, "/EXPORT:SimConnect_RequestSystemState")
#pragma comment(linker, "/EXPORT:SimConnect_RequestDataOnSimObject")
#pragma comment(linker, "/EXPORT:SimConnect_GetNextDispatch")
#pragma comment(linker, "/EXPORT:SimConnect_MapClientEventToSimEvent")
#pragma comment(linker, "/EXPORT:SimConnect_TransmitClientEvent_EX1")
#define API SIMCONNECTAPI
API SimConnect_Open(HANDLE* handle, LPCSTR, HWND, DWORD, HANDLE, DWORD) {
    const auto path = std::getenv("RFI_TIMING_FIXTURE");
    if (!path) return E_INVALIDARG;
    root = path;
    if (std::getenv("RFI_TIMING_PRIORITY")) {
        if (!SetPriorityClass(GetCurrentProcess(), ABOVE_NORMAL_PRIORITY_CLASS)) return E_FAIL;
    }
    if (const auto m = std::getenv("RFI_TIMING_MODE")) mode = m;
    std::ifstream(root) >> generation;
    std::ofstream(root) << ++generation;
    started = GetTickCount64();
    *handle = reinterpret_cast<HANDLE>(1);
    SIMCONNECT_RECV_OPEN opened{};
    std::strcpy(opened.szApplicationName, "RFI TEST ONLY timing fixture");
    opened.dwApplicationVersionMajor = 1; opened.dwApplicationVersionMinor = 38;
    opened.dwSimConnectVersionMajor = 10;
    enqueue(opened, SIMCONNECT_RECV_ID_OPEN);
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
    const auto at = GetTickCount64();
    if (mode == "switch" && generation < 3 && !switched && at - started >= 4000) {
        switched = true;
        SIMCONNECT_RECV_EVENT e{}; e.uEventID = 3;
        enqueue(e, SIMCONNECT_RECV_ID_EVENT);
    }
    const double t = double(at - started) / 1000.;
    std::vector<char> bytes(168);
    auto p = reinterpret_cast<SIMCONNECT_RECV_SIMOBJECT_DATA*>(bytes.data());
    p->dwSize = 168; p->dwVersion = 1; p->dwID = SIMCONNECT_RECV_ID_SIMOBJECT_DATA;
    p->dwRequestID = id; p->dwObjectID = 100 + generation; p->dwDefineID = 2;
    p->dwDefineCount = 16; p->dwentrynumber = 1; p->dwoutof = 1;
    // Values represent the REQUEST instant, not the later dispatch instant.
    const double altitude = 2000. * generation + 2000. * std::sin(t / 3.);
    double values[] = {altitude, altitude, 29.92, 0.,
        -25. * std::sin(t * 2.5), -65. * std::sin(t * 1.5), 0.,
        110. + 25. * std::sin(t), 0., std::fmod(t * 5., 360.), 0.,
        1500. * std::sin(t / 2.), 0., 6. * std::sin(t * 2.),
        128. * .8 * std::sin(t * 1.1), 0.};
    if (mode == "failed" && at - started >= 4700 && at - started < 5700) {
        values[6] = 1.; values[10] = 1.; values[12] = 1.; values[15] = 1.; // Explicit instrument failure, not delay.
    }
    std::memcpy(bytes.data() + 40, values, sizeof(values));
    const unsigned delays[] = {15, 25, 35, 20, 30, 40, 10, 35};
    unsigned delay = delays[requests++ % 8];
    if (mode == "pulse" && (at - started) / 8000 > pulse_bucket) {
        pulse_bucket = static_cast<unsigned>((at - started) / 8000);
        delay = 250; // Repeated short interruptions for the human visual check.
    }
    if ((mode == "gap" || mode == "short_gap") && !delayed && at - started >= 4700) {
        delayed = true; delay = mode == "gap" ? 800 : 250;
        // A short interruption should repeat pixels; a long one must invalidate.
    }
    queue.push_back({at + delay, std::move(bytes)});
    return S_OK;
}
API SimConnect_GetNextDispatch(HANDLE, SIMCONNECT_RECV** data, DWORD* bytes) {
    const auto now = GetTickCount64();
    for (auto i = queue.begin(); i != queue.end(); ++i) {
        if (i->due > now) continue;
        current = std::move(i->bytes); queue.erase(i);
        *data = reinterpret_cast<SIMCONNECT_RECV*>(current.data());
        *bytes = static_cast<DWORD>(current.size()); return S_OK;
    }
    return E_FAIL;
}
API SimConnect_MapClientEventToSimEvent(HANDLE, SIMCONNECT_CLIENT_EVENT_ID, const char*) {
    write_attempt(); return E_NOTIMPL;
}
API SimConnect_TransmitClientEvent_EX1(HANDLE, SIMCONNECT_OBJECT_ID, SIMCONNECT_CLIENT_EVENT_ID, SIMCONNECT_NOTIFICATION_GROUP_ID, SIMCONNECT_EVENT_FLAG, DWORD, DWORD, DWORD, DWORD, DWORD) {
    write_attempt(); return E_NOTIMPL;
}
