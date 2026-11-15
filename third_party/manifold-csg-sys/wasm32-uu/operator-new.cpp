// operator-new.cpp: global operator new/delete for the browser build, replacing wasm-cxx-shim's
// (libcxx/src/operator_new_delete.cpp), whose throwing `operator new` is `if (!p) __builtin_trap()`: a failed allocation of
// the C++ runtime (a std::vector, a std::unordered_map node, a std::string) stopped the module with an `unreachable` and no
// words.  Manifold's own `Vec` allocations are checked by the carried patch 0004 and say what asked for how much; these now
// say the same: a failing `operator new` calls `manifold_alloc_failed` (manifold-csg-sys/src/lib.rs, which panics with the
// stage, the size and the module's memory, and the engine's panic hook puts it on the console before the trap).
//
// All of the shim's symbols are defined here, and a few it lacks (the array and aligned array forms, the array nothrow
// forms), so the linker never needs the shim's member and its definitions are not a second copy: this archive comes before
// the shim's on the link line, and a symbol both archives provide is taken from the first.
//
// Like the shim's, this file includes no libc++ header (`<new>` here would couple it to libc++'s declarations): the
// `std::nothrow_t` and `std::align_val_t` below mangle to the same names.

#include <stddef.h>  // size_t

extern "C" {
    void* malloc(size_t);
    void  free(void*);
    void* aligned_alloc(size_t alignment, size_t size);
    // Defined by the program (manifold-csg-sys): reports that an allocation of `bytes` failed, and stops. It does not return.
    [[noreturn]] void manifold_alloc_failed(size_t bytes);
}

namespace std {
    struct nothrow_t {};
    enum class align_val_t : size_t {};
}

// ---- operator new, throwing forms: no exception runtime, so a failure is said and stops ----

void* operator new(size_t size) {
    void* p = malloc(size);
    if (!p) manifold_alloc_failed(size);
    return p;
}

void* operator new[](size_t size) {
    return ::operator new(size);
}

void* operator new(size_t size, std::align_val_t align) {
    void* p = aligned_alloc(static_cast<size_t>(align), size);
    if (!p) manifold_alloc_failed(size);
    return p;
}

void* operator new[](size_t size, std::align_val_t align) {
    return ::operator new(size, align);
}

// ---- operator new, nothrow forms: the standard's contract is a null for the caller to handle, so they return one ----

void* operator new(size_t size, const std::nothrow_t&) noexcept {
    return malloc(size);
}

void* operator new[](size_t size, const std::nothrow_t&) noexcept {
    return malloc(size);
}

void* operator new(size_t size, std::align_val_t align, const std::nothrow_t&) noexcept {
    return aligned_alloc(static_cast<size_t>(align), size);
}

void* operator new[](size_t size, std::align_val_t align, const std::nothrow_t&) noexcept {
    return aligned_alloc(static_cast<size_t>(align), size);
}

// ---- operator delete: free(nullptr) is well-defined; the sized forms ignore the size ----

void operator delete  (void* p) noexcept                                  { free(p); }
void operator delete[](void* p) noexcept                                  { free(p); }
void operator delete  (void* p, std::align_val_t) noexcept                { free(p); }
void operator delete[](void* p, std::align_val_t) noexcept                { free(p); }
void operator delete  (void* p, size_t) noexcept                          { free(p); }
void operator delete[](void* p, size_t) noexcept                          { free(p); }
void operator delete  (void* p, size_t, std::align_val_t) noexcept        { free(p); }
void operator delete[](void* p, size_t, std::align_val_t) noexcept        { free(p); }

// ---- the probe: a test's way to make the C++ runtime's operator new fail ----
//
// `cargo xtask parity --too-big` calls it (through the engine's `probe_operator_new`) to see that a failing operator new
// says its size and the module's memory, as a failing Manifold allocation does.  `::operator new` called as a function is
// not a new-expression, so the compiler may not elide it.
// The pointer goes through a volatile, so the optimiser cannot fold the allocation and its release away (it may, for a pair of
// allocation functions it knows, even when they are called by name).
static void* volatile probe_sink;
extern "C" void manifold_csg_probe_operator_new(size_t bytes) {
    probe_sink = ::operator new(bytes);
    ::operator delete(probe_sink);
}
