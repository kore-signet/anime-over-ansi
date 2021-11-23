#  block spec
```
[u64 packet byte length]
[u8 compression marker [0 = Nothing; 1 = zstd]]
[u64 uncompressed size, not present without the compression marker] 
[u32 stream index]
[u32 ADLER32 checksum]
[u64 presentation timestamp, nanoseconds]
[u64 duration, nanoseconds; set to u64::MAX to indicate None]
```
(total 33 bytes (41 if zstd compression is on))