# Goal

Process payment with a P99 of 5ms

# Optimizations

- https://deterministic.space/high-performance-rust.html
- https://deterministic.space/secret-life-of-cows.html
- https://likebike.com/posts/How_To_Write_Fast_Rust_Code.html
- http://troubles.md/posts/rustfest-2018-workshop/
- https://nnethercote.github.io/perf-book/build-configuration.html

# Performance Tests Results

| Commit SHA | Timestamp | P99 (ms) | Success Requests | Failed Requests | Lag | Score |
|------------|-----------|----------|------------------|-----------------|-----|-------|
| [6eb13d6](https://github.com/josimar-silva/rinha-de-backend-2025/commit/6eb13d67e4905b88eeec17f9025b3fd72b1378b4) | 2025-07-25T13:53:29Z | 60.24655469999998ms | 7337 | 9551 | 7337 | 0 |
