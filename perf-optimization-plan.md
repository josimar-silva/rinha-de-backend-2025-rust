# Performance Optimization Plan

## Goal

The primary goal is to achieve a P99 (99th percentile) latency of 5ms for payment processing within the application.

## Summary of Performance Research

Based on the provided references, here's a consolidated summary of key Rust optimization tips:

### Compiler and Build Configuration
*   **Link-Time Optimization (LTO)**: Setting `lto = "fat"` in `Cargo.toml` enables whole-program optimization, allowing the compiler to perform more aggressive optimizations across crate boundaries.
*   **Codegen Units**: Setting `codegen-units = 1` in `Cargo.toml` reduces the number of parallel compilation units, leading to more thorough optimization by the compiler, though it can increase compile times.
*   **Target CPU Optimization**: Using `RUSTFLAGS="-C target-cpu=native"` instructs the compiler to generate code specifically optimized for the CPU where the compilation is performed, leveraging specific CPU features.
*   **Panic Strategy**: Setting `panic = "abort"` in `Cargo.toml` (for release builds) causes the program to immediately terminate on a panic, avoiding the overhead of stack unwinding. This reduces binary size and can improve performance, but means no recovery from panics.
*   **Profile-Guided Optimization (PGO)**: An advanced technique where the compiler uses data from a previous run of the program to optimize subsequent builds, leading to significant runtime improvements.

### Memory Management and Data Structures
*   **Alternative Memory Allocators**: Replacing the default system allocator with specialized allocators like `jemalloc` or `mimalloc` can improve memory allocation and deallocation performance, especially in high-throughput applications.
*   **`std::borrow::Cow` (Clone-on-Write)**: This type allows for efficient data reuse by abstracting between borrowed and owned data, avoiding unnecessary copying and allocations when data is not modified.
*   **Reducing Heap Allocations**: Minimizing dynamic memory allocations by using techniques like `Rc<String>` for shared, immutable data, or pre-allocated buffers/slabs.
*   **Optimizing `HashMap` Performance**: If `HashMap` operations are a bottleneck, consider switching to faster hashing algorithms like `FnvHash` or `fxhash`.
*   **Infallible Data Structures**: Designing data structures that are always valid by construction can eliminate the need for runtime validity checks, reducing overhead.

### Profiling and Benchmarking
*   **Measure with Release Builds**: Always measure performance using release builds, as debug builds include optimizations that can significantly impact performance.
*   **Profiling Tools**: Use profiling tools like `perf` (on Linux), Valgrind (specifically Callgrind), and KCachegrind to identify performance bottlenecks and hot spots in the code.
*   **Benchmarking**: Utilize tools like `cargo benchcmp` to systematically verify the performance impact of changes and ensure optimizations are effective.

### Code-Level Optimizations
*   **Inlining Functions**: Judiciously applying `#[inline]` or `#[inline(always)]` attributes to small, frequently called functions in performance-critical paths can reduce function call overhead.
*   **Minimizing Redundant Work**: Optimizing logic to avoid intermediate data structures and unnecessary computations.
*   **Safe Indexing**: Using `if let Some(v) = a.get(i)` for indexing can sometimes lead to better compiler optimizations by avoiding bounds checks if the compiler can prove safety.

## Overall Optimization Plan

Our optimization strategy will follow a phased approach, prioritizing easy wins and data-driven decisions.

### Phase 1: Easy Wins & Setup for Profiling

1.  **Apply Build Configuration Optimizations**:
    *   Set `panic = "abort"` in `Cargo.toml` for release profiles. (Already applied)
    *   Add `RUSTFLAGS="-C target-cpu=native"` to build commands in `justfile` and `Dockerfile`. (Already applied)
2.  **Set up a Profiling Workflow**:
    *   Integrate `perf` into the GitHub Actions `perf-tests.yaml` workflow.
    *   Run the application under load with `perf record` to collect profiling data (`perf.data`).
    *   Upload `perf.data` as a workflow artifact for local analysis.
    *   Continue using `k6` for high-level performance metrics in `perf.md`.

### Phase 2: Profile-Driven Code Optimizations

1.  **Run Profiler and Analyze Data**: Execute the application with the profiling setup and analyze the generated `perf.data` to identify the most significant performance bottlenecks (hot spots).
2.  **Targeted Code Optimizations**: Based on profiling results, apply specific code-level optimizations:
    *   Apply `#[inline]` to hot functions.
    *   Investigate and implement `std::borrow::Cow` where applicable to reduce allocations.
    *   Review `HashMap` usage and consider alternative hashing algorithms if they are identified as bottlenecks.
    *   Refactor code to minimize memory operations and avoid unnecessary intermediate data structures.

### Phase 3: Advanced Optimizations (If Necessary)

1.  **Alternative Memory Allocators**: If profiling indicates memory allocation is a significant bottleneck, explore integrating `jemalloc` or `mimalloc`.
2.  **Profile-Guided Optimization (PGO)**: If further significant performance gains are required, implement PGO for the most aggressive compiler optimizations.

## Current Status

*   `lto = "fat"` and `codegen-units = 1` are configured in `Cargo.toml`.
*   `panic = "abort"` has been added to `Cargo.toml`.
*   `RUSTFLAGS="-C target-cpu=native"` has been added to `justfile` and `Dockerfile`.

## Next Steps

The immediate next step is to integrate `perf` into the `perf-tests.yaml` GitHub Actions workflow to enable profiling and data collection.
