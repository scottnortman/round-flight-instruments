// Narrow ABI bridge compiled against the installed Microsoft header.
// No SDK sources or binaries are redistributed by this project.
#include <windows.h>
#pragma warning(push)
#pragma warning(disable: 4245) // SDK 0.24.5 intentionally assigns -1 to DWORD constants.
#include <SimConnect.h>
#pragma warning(pop)
#include <cstdint>
#include <cstring>
#include <new>
#include <cstdio>

struct RfiEvent {
    uint32_t kind, request, code, detail;
    double values[16]; // Six-pack definition; explicit SDK units, copied before next dispatch.
};
static_assert(sizeof(RfiEvent) == 144, "Rust/C event layout changed");
static_assert(sizeof(SIMCONNECT_RECV_SIMOBJECT_DATA) == 44, "Unexpected SDK packet layout");
struct Client {
    HMODULE dll = nullptr;
    HANDLE connection = nullptr;
    decltype(&SimConnect_Open) open = nullptr;
    decltype(&SimConnect_Close) close = nullptr;
    decltype(&SimConnect_AddToDataDefinition) define = nullptr;
    decltype(&SimConnect_SubscribeToSystemEvent) subscribe = nullptr;
    decltype(&SimConnect_RequestDataOnSimObject) request = nullptr;
    decltype(&SimConnect_RequestSystemState) state = nullptr;
    decltype(&SimConnect_GetNextDispatch) next = nullptr;
    decltype(&SimConnect_MapClientEventToSimEvent) map_event = nullptr;
    decltype(&SimConnect_TransmitClientEvent_EX1) transmit = nullptr;
    ~Client() {
        if (connection && close) close(connection);
        if (dll) FreeLibrary(dll);
    }
};
// GetProcAddress returns an untyped function address; memcpy avoids C4191 while
// preserving the exact calling convention/types from the SDK declaration.
template<typename T> bool load(HMODULE dll, const char* name, T& out) {
    auto address = GetProcAddress(dll, name);
    static_assert(sizeof(address) == sizeof(out), "Function pointer size");
    std::memcpy(&out, &address, sizeof(out));
    return address != nullptr;
}
extern "C" void* rfi_sc_open(const wchar_t* path, uint32_t* stage, uint32_t* error) {
    *stage = 1; *error = ERROR_NOT_ENOUGH_MEMORY;
    auto c = new (std::nothrow) Client();
    if (!c) return nullptr;
    c->dll = LoadLibraryExW(path, nullptr, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (!c->dll) { *error = GetLastError(); delete c; return nullptr; }
    *stage = 2; *error = ERROR_PROC_NOT_FOUND;
    if (!(load(c->dll,"SimConnect_Open",c->open) && load(c->dll,"SimConnect_Close",c->close)
        && load(c->dll,"SimConnect_AddToDataDefinition",c->define)
        && load(c->dll,"SimConnect_SubscribeToSystemEvent",c->subscribe)
        && load(c->dll,"SimConnect_RequestDataOnSimObject",c->request)
        && load(c->dll,"SimConnect_RequestSystemState",c->state)
        && load(c->dll,"SimConnect_GetNextDispatch",c->next))) { delete c; return nullptr; }
    *stage = 3;
    auto hr = c->open(&c->connection, "Round Flight Instruments", nullptr, 0, nullptr, SIMCONNECT_OPEN_CONFIGINDEX_LOCAL);
    if (FAILED(hr)) { *error = static_cast<uint32_t>(hr); delete c; return nullptr; }
    *stage = 4;
    const char* names[] = {"INDICATED ALTITUDE", "PRESSURE ALTITUDE", "KOHLSMAN SETTING HG:1", "PARTIAL PANEL ALTIMETER",
        "ATTITUDE INDICATOR PITCH DEGREES", "ATTITUDE INDICATOR BANK DEGREES", "PARTIAL PANEL ATTITUDE",
        "AIRSPEED INDICATED", "PARTIAL PANEL AIRSPEED", "PLANE HEADING DEGREES MAGNETIC", "PARTIAL PANEL HEADING",
        "VERTICAL SPEED", "PARTIAL PANEL VERTICAL VELOCITY", "TURN INDICATOR RATE", "TURN COORDINATOR BALL", "PARTIAL PANEL TURN COORDINATOR"};
    const char* units[] = {"feet", "feet", "inHg", "enum", "degrees", "degrees", "enum",
        "knots", "enum", "degrees", "enum", "feet per minute", "enum", "degrees per second", "position 128", "enum"};
    for (unsigned i = 0; i < 16; ++i) {
        hr = c->define(c->connection, 2, names[i], units[i], SIMCONNECT_DATATYPE_FLOAT64, 0, SIMCONNECT_UNUSED);
        if (FAILED(hr)) { *error = static_cast<uint32_t>(hr); delete c; return nullptr; }
    }
    const char* events[] = {"Sim", "Pause_EX1", "AircraftLoaded", "FlightLoaded", "Crashed", "PositionChanged", "CrashReset"};
    for (unsigned i = 0; i < 7; ++i) {
        hr = c->subscribe(c->connection, i + 1, events[i]);
        if (FAILED(hr)) { *error = static_cast<uint32_t>(hr); delete c; return nullptr; }
    }
    *stage = 0; *error = 0; return c;
}
extern "C" void rfi_sc_close(void* client) { delete static_cast<Client*>(client); }
extern "C" uint64_t rfi_sc_uptime_ms() { return GetTickCount64(); }
extern "C" int32_t rfi_sc_set_baro(void* client, uint32_t pressure16, uint64_t expires_ms) {
    auto c = static_cast<Client*>(client);
    if (GetTickCount64() >= expires_ms) return E_ABORT;
    // Narrow optional write path; read-only connections never load/map it.
    if (!c->transmit) {
        if (!load(c->dll,"SimConnect_MapClientEventToSimEvent",c->map_event)
            || !load(c->dll,"SimConnect_TransmitClientEvent_EX1",c->transmit)) return E_NOINTERFACE;
        auto hr = c->map_event(c->connection, 100, "KOHLSMAN_SET");
        if (FAILED(hr)) { c->transmit = nullptr; return hr; }
    }
    if (pressure16 < 15225 || pressure16 > 16797) return E_INVALIDARG;
    if (GetTickCount64() >= expires_ms) return E_ABORT;
    return c->transmit(c->connection, SIMCONNECT_OBJECT_ID_USER, 100,
        SIMCONNECT_GROUP_PRIORITY_HIGHEST, SIMCONNECT_EVENT_FLAG_GROUPID_IS_PRIORITY,
        pressure16, 1, 0, 0, 0); // Value in hPa*16, explicitly primary altimeter index 1.
}
extern "C" int32_t rfi_sc_request(void* client, uint32_t id, uint32_t heartbeat) {
    auto c = static_cast<Client*>(client);
    return heartbeat ? c->state(c->connection, id, "Sim")
        : c->request(c->connection, id, 2, SIMCONNECT_OBJECT_ID_USER, SIMCONNECT_PERIOD_ONCE, 0, 0, 0, 0);
}
// 0=empty, 1=event copied, negative=invalid packet/API failure. Pointer lifetime
// belongs to the SDK; copy all used fields before the next dispatch call.
static int32_t decode(SIMCONNECT_RECV* p, DWORD bytes, RfiEvent* out, DWORD definition = 1) {
    if (!p || bytes < sizeof(SIMCONNECT_RECV) || p->dwSize > bytes || p->dwSize < sizeof(SIMCONNECT_RECV)) return E_UNEXPECTED;
    *out = {};
    switch (p->dwID) {
    case SIMCONNECT_RECV_ID_OPEN: out->kind = 1; break;
    case SIMCONNECT_RECV_ID_QUIT: out->kind = 3; break;
    case SIMCONNECT_RECV_ID_EXCEPTION: {
        if (p->dwSize < sizeof(SIMCONNECT_RECV_EXCEPTION)) return E_UNEXPECTED;
        auto e = static_cast<SIMCONNECT_RECV_EXCEPTION*>(p);
        out->kind = 2; out->code = e->dwException; out->request = e->dwSendID; out->detail = e->dwIndex; break;
    }
    case SIMCONNECT_RECV_ID_EVENT:
    case SIMCONNECT_RECV_ID_EVENT_FILENAME: {
        if (p->dwSize < sizeof(SIMCONNECT_RECV_EVENT)) return E_UNEXPECTED;
        auto e = static_cast<SIMCONNECT_RECV_EVENT*>(p);
        out->kind = 4; out->code = e->uEventID; out->detail = e->dwData; break;
    }
    case SIMCONNECT_RECV_ID_SYSTEM_STATE: {
        if (p->dwSize < sizeof(SIMCONNECT_RECV_SYSTEM_STATE)) return E_UNEXPECTED;
        auto s = static_cast<SIMCONNECT_RECV_SYSTEM_STATE*>(p);
        out->kind = 6; out->request = s->dwRequestID; out->code = s->dwInteger; break;
    }
    case SIMCONNECT_RECV_ID_SIMOBJECT_DATA: {
        const size_t offset = sizeof(SIMCONNECT_RECV_SIMOBJECT_DATA) - sizeof(DWORD);
        const size_t count = definition == 2 ? 16 : 4;
        if (p->dwSize < offset + count * sizeof(double)) return E_UNEXPECTED;
        auto s = static_cast<SIMCONNECT_RECV_SIMOBJECT_DATA*>(p);
        if (s->dwDefineID != definition || s->dwDefineCount != count || s->dwFlags != 0
            || s->dwentrynumber != 1 || s->dwoutof != 1) return E_UNEXPECTED;
        // USER is a request selector, not the resolved object ID in a reply.
        // Rust binds this ID only after matching a live USER request and its age.
        out->kind = 5; out->request = s->dwRequestID; out->code = s->dwObjectID;
        std::memcpy(out->values, reinterpret_cast<const char*>(p) + offset, count * sizeof(double)); break;
    }
    default: break;
    }
    return 1;
}
// Offline fixture seam: preserve captured bytes and select their definition ID.
extern "C" int32_t rfi_sc_decode_fixture(const unsigned char* input, uint32_t size, uint32_t definition, RfiEvent* out) {
    alignas(8) unsigned char copy[512] = {};
    if (!input || size > sizeof(copy)) return E_UNEXPECTED;
    std::memcpy(copy, input, size);
    return decode(reinterpret_cast<SIMCONNECT_RECV*>(copy), size, out, definition);
}
extern "C" int32_t rfi_sc_poll(void* client, RfiEvent* out) {
    auto c = static_cast<Client*>(client);
    SIMCONNECT_RECV* p = nullptr; DWORD bytes = 0;
    auto hr = c->next(c->connection, &p, &bytes);
    if (hr == E_FAIL) return 0; // SDK also uses E_FAIL when its queue is empty.
    if (FAILED(hr)) return hr;
    const auto result = decode(p, bytes, out, 2);
    if (result < 0) {
        // One terminal diagnostic, not a per-poll trace. Read only bounded fields.
        std::fprintf(stderr, "Local SimConnect packet validation rejected: bytes=%lu", bytes);
        if (p && bytes >= sizeof(SIMCONNECT_RECV)) {
            std::fprintf(stderr, " size=%lu version=%lu id=%lu", p->dwSize, p->dwVersion, p->dwID);
            if (p->dwID == SIMCONNECT_RECV_ID_SIMOBJECT_DATA && bytes >= 40 && p->dwSize >= 40) {
                auto s = static_cast<SIMCONNECT_RECV_SIMOBJECT_DATA*>(p);
                std::fprintf(stderr, " request=%lu definition=%lu object=%lu count=%lu flags=%lu entry=%lu/%lu",
                    s->dwRequestID, s->dwDefineID, s->dwObjectID, s->dwDefineCount, s->dwFlags, s->dwentrynumber, s->dwoutof);
            }
        }
        std::fprintf(stderr, "\n");
        return 2; // Distinct from raw negative SDK HRESULTs.
    }
    return result;
}
// Packet fixtures exercise the actual native decoder against this SDK's layout.
// No fake SDK DLL or simulated-data mode is linked into the provider.
extern "C" uint32_t rfi_sc_decoder_test() {
    alignas(8) unsigned char bytes[72] = {};
    auto p = reinterpret_cast<SIMCONNECT_RECV_SIMOBJECT_DATA*>(bytes);
    p->dwSize = sizeof(bytes); p->dwID = SIMCONNECT_RECV_ID_SIMOBJECT_DATA;
    p->dwRequestID = 42; p->dwDefineID = 1; p->dwDefineCount = 4;
    p->dwentrynumber = 1; p->dwoutof = 1;
    const double values[] = {8500.0, 8300.0, 30.12, 0.0};
    std::memcpy(bytes + 40, values, sizeof(values));
    RfiEvent out = {};
    if (decode(p, sizeof(bytes), &out) != 1 || out.kind != 5 || out.request != 42 || std::memcmp(out.values, values, sizeof(values))) return 1;
    if (decode(p, 71, &out) >= 0) return 2;
    p->dwSize = 71;
    if (decode(p, 71, &out) >= 0) return 3;
    p->dwSize = sizeof(bytes); p->dwDefineCount = 3;
    if (decode(p, sizeof(bytes), &out) >= 0) return 4;
    p->dwDefineCount = 4; p->dwObjectID = 1;
    if (decode(p, sizeof(bytes), &out) != 1 || out.code != 1) return 5;
    if (decode(nullptr, 0, &out) >= 0) return 6;
    SIMCONNECT_RECV_EVENT event = {};
    event.dwSize = sizeof(event); event.dwID = SIMCONNECT_RECV_ID_EVENT;
    event.uEventID = 2; event.dwData = 4;
    if (decode(&event, sizeof(event), &out) != 1 || out.kind != 4 || out.code != 2 || out.detail != 4) return 7;
    event.dwSize = sizeof(event) - 1;
    if (decode(&event, sizeof(event), &out) >= 0) return 8;
    return 0;
}
