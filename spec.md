## file metadata

An ansi.moe file should start with:

- a little-endian 8 byte integer, describing the length in bytes of the serialized metadata
- a messagepack-encoded blob: the metadata!

the metadata is a structure defined as such:

```rust
struct VideoMetadata {
    pub video_tracks: Vec<VideoTrack>, // list of video tracks in the file
    pub subtitle_tracks: Vec<SubtitleTrack>, // list of subtitle tracks in the file
    pub attachments: Vec<Attachment>, // list of attachments to the file
    pub compression: CompressionMode, // compression mode of this file: 0 = None; 1 = Zstd-compressed
}

```

where VideoTrack is:

```rust
 struct VideoTrack {
    pub name: Option<String>, // optional name for the track
    pub color_mode: ColorMode, // what color mode does the track use: 0 = True; 1 = 8bit
    pub height: u32,           // height in pixels (divide by two to get line count for terminal)
    pub width: u32,            // width in pixels
    pub codec_private: Option<Vec<u8>>, // data you may need to use to decode this track's packet data (currently unused)
    pub index: u16 // index the packets of this track will have
}
```

SubtitleTrack is: 
```rust
struct SubtitleTrack {
    pub name: Option<String>, // optional name for the track
    pub lang: Option<String>, // optional language for the track
    pub format: SubtitleFormat, // format for the subtitles (currently an enum: SubRip | SubStationAlpha | Unknown(String))
    pub codec_private: Option<Vec<u8>>, // data you may need to use to decode this track's packet data (used in SSA tracks to store the SSA header!)
    pub index: u16, // index the packets of this track will have
}
```

and Attachment is:

```rust
enum Attachment {
    Binary(Vec<u8>),
    Midi(Vec<u8>),
}
```

what follows is the video's packets..

## packets

### Decoded packet

A decoded ansi.moe packet - the ones you want to get from your demuxer - should probably look something like this:

- the index of the stream it belongs to
- the time the packet's contents should be shown by the player
- how long it should be shown for
- a key-value map of extra parameters for this codec: both keys and values are stored as 4byte unsigned integers.
- the binary data of the packet

In Rust, it could look like this:

```rust
struct Packet {
    stream_index: u16,
    presentation_length: Duration,
    presentation_time: Duration,
    data: Vec<u8>,
    extra_data: BTReeMap<u32, u32>,
}
```

### Encoded packet

The encoded - or wire - form of an ansi.moe packet is a bit more strictly defined. This is what you'll find stored in .ansi files or network streams, and need to transform to the decoded packets.

It's composed of a header - defined as a C-ABI struct - and a binary blob.

*note: all fields are little endian unsigned integers!*


| name                | bytes | type | description                          |
| --------------------- | :------ | ------ | -------------------------------------- |
| stream_index        | 2     | u16  | stream this belongs to               |
| checksum            | 4     | u32  | CRC32 checksum                       |
| presentation_time   | 8     | u64  | presentation timestamp (nanoseconds) |
| presentation_length | 8     | u64  | presentation length (nanoseconds)    |
| data_length         | 8     | u64  | length of packet data                |
| extra_data_length   | 2     | u16  | length of packet extra-data map      |

The binary blob is composed of:

- a byte string `extra_data_length` bytes long; interpreted as a list of (key: u32, value: u32) pairs
- a byte string `data_length` bytes long; the packet's data itself.

### Extra Data flags

Here's some flags you may find in the extra_data map:


| Key (string repr.) | Key (hexadecimal) | value type                                         |
| -------------------- | ------------------- | ---------------------------------------------------- |
| vidf               | 66646976          | video packet flags                                 |
| zstl               | 6c74737a          | zstd - uncompressed data length, u32 little endian |


