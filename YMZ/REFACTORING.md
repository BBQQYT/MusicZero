# Refactoring roadmap

## Playback

- [ ] Streaming audio instead of downloading the complete file before playback
- [ ] Bounded playback buffer
- [ ] Gapless transition between tracks
- [ ] Explicit playback state
- [ ] Centralized player state
- [ ] Correct MPRIS position/state updates

## Network

- [ ] Shared HTTP client
- [ ] Connection timeout
- [ ] Request timeout
- [ ] Retry with exponential backoff
- [ ] Jitter
- [ ] HTTP 429 handling
- [ ] HTTP 5xx handling
- [ ] 401/403 handling
- [ ] Cancellation of stale requests

## Cache

- [ ] Disk cache
- [ ] LRU eviction
- [ ] Configurable cache size
- [ ] Metadata cache
- [ ] Artwork cache
- [ ] Audio cache
- [ ] Avoid duplicate downloads

## Prefetch

- [ ] Prefetch next track
- [ ] Optional next+1 metadata
- [ ] Bounded concurrency
- [ ] Cancellation
- [ ] Do not prefetch duplicate tracks

## Reliability

- [ ] Remove panic-prone unwrap/expect from network/API paths
- [ ] Typed errors
- [ ] Structured logging
- [ ] Graceful shutdown
- [ ] Retry classification

## Configuration

- [ ] TOML configuration
- [ ] Environment overrides where useful
- [ ] Cache size
- [ ] Prefetch count
- [ ] Network timeout
- [ ] Retry count
- [ ] Audio quality

## Testing

- [ ] Queue tests
- [ ] Cache tests
- [ ] Retry tests
- [ ] API parsing tests
- [ ] Playback state tests
- [ ] MPRIS tests
- [ ] Network failure tests
- [ ] HTTP 429 tests
- [ ] HTTP 5xx tests
- [ ] Connection reset tests

## Performance

Target:

- startup: < 500 ms where practical
- next-track gap: < 100 ms when the next track is prefetched
- idle RSS: keep close to the existing project target
- bounded memory usage
