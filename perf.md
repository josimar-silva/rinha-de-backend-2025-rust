# Goal

Process payment with a P99 of 5ms

# Optimizations

- https://deterministic.space/high-performance-rust.html
- https://deterministic.space/secret-life-of-cows.html
- https://likebike.com/posts/How_To_Write_Fast_Rust_Code.html
- http://troubles.md/posts/rustfest-2018-workshop/
- https://nnethercote.github.io/perf-book/build-configuration.html

# Performance Tests Results

| Commit SHA | Timestamp | Iterations | Avg Req Duration (ms) | P95 Req Duration (ms) | P99 Req Duration (ms) | HTTP Fail Rate |
|------------|-----------|------------|-----------------------|-----------------------|-----------------------|----------------|