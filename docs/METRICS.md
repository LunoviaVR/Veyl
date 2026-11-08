# Metric definitions

Exact definitions of every derived number shown by PC Health Console. The
implementations are in `crates/core/src/frame_stats.rs`, `crates/core/src/rate.rs`
and `crates/windows-platform/src/gpu.rs`, each with unit tests.

## Frame statistics (Gaming)

Input: per-frame times in milliseconds for the game's main swap chain, as reported
by PresentMon (`MsBetweenPresents`, or `FrameTime` in v2 metrics). If a process
presents to several swap chains, only the one with the most frames is analysed;
the others are counted and reported as excluded. Non-finite and non-positive frame
times are discarded and counted.

| Metric | Definition |
|---|---|
| Average FPS | frames ÷ total time = `1000 / mean(frame time)`. Not the mean of per-frame FPS, which overweights fast frames. |
| Current FPS | frames ÷ time over the newest frames that together span at least 1000 ms. |
| Pn frame time | Nearest-rank percentile of the ascending-sorted frame times: rank = ⌈n × p/100⌉, value = sorted[rank − 1]. |
| 1% low FPS | `1000 / mean` of the slowest ⌈n × 1%⌉ frame times (at least one frame). |
| 0.1% low FPS | Same with 0.1%. |
| Stutter frame | A frame longer than *k* × the median frame time of the analysed window (*k* = 2.5 by default, configurable 1.5–10). |
| Stutter time % | Sum of stutter-frame times ÷ total time × 100. |
| Pacing variation | Mean absolute difference between consecutive frame times. Perfectly even pacing scores 0 at any frame rate. |
| Standard deviation | Population standard deviation of frame times. |

Live values are computed over the newest 30 seconds of frames, refreshed twice per
second. Saved sessions are computed over every recorded frame.

### Bottleneck hint

Session details show a hint derived from recorded GPU utilization only: ≥ 95 % average
"most likely GPU-limited"; < 75 % "something else limited the frame rate (commonly
CPU main thread, frame cap or V-Sync)". It is labelled as a hint, never a diagnosis.

## CPU

| Metric | Source |
|---|---|
| Utilization | PDH `\Processor Information(_Total)\% Processor Time` — time-based busy share. |
| Utility | PDH `% Processor Utility` — busy share scaled by actual vs. base frequency (Task Manager's number). Can exceed 100. |
| Effective clock | `Processor Frequency × % Processor Performance / 100`, averaged over the sample interval. |
| Per-process CPU | Δ(kernel + user time) ÷ (elapsed × logical processors). Time-based, like Task Manager's Details tab; reads lower than the frequency-scaled Processes tab under boost. |

## GPU (Task Manager's rule)

Per engine (LUID, physical index, engine ordinal): sum of all processes' `Utilization
Percentage`. Adapter utilization = busiest engine (clamped to 100). Process GPU =
the busiest engine that process uses. Engines of the same type are separate
hardware and are never summed together.

## Rates

Every rate is Δcounter ÷ the measured monotonic time between the two readings that
produced the delta. A counter that goes backwards (driver reset, reused PID) yields
"not measured" for that interval, never a negative or wrapped value.

## History

Samples are aggregated into 5-second buckets (avg/min/max) and rolled up into
1-minute buckets. Charts mark a gap where consecutive buckets are more than 2.5
bucket widths apart: no data was recorded there, and nothing is interpolated.
