# kudo wire protocol, v1 draft

## scope

kudo moves a video file from a sender to a receiver over a reliable,
ordered byte stream, in chunks, such that the receiver can begin
playback before the transfer completes. The protocol does not know or
care what transport carries it. v1 runs over Bluetooth RFCOMM.

Both peers implement this spec independently (Rust on Linux, Kotlin
on Android). This document is the contract.

## conventions

All integers are unsigned big-endian. Strings are UTF-8, always
length-prefixed, never null-terminated. One side is the sender (has
the file), one is the receiver (plays it). Which peer initiated the
underlying connection is irrelevant to these roles.

## framing

Every message is a frame:

    +----------+----------+------------------+
    | len: u32 | type: u8 | payload: len-1 B |
    +----------+----------+------------------+

len counts everything after the len field itself (so type + payload).
Maximum frame size is 131072 bytes (128 KiB payload + header). A peer
receiving a frame with len larger than this must send ERROR and close.
A frame with an unknown type must be answered with ERROR code 2 and
the connection closed. There are no optional or ignorable frames in v1.

## message types

    0x01 HELLO          both directions, handshake
    0x02 OFFER          sender -> receiver, file metadata
    0x03 ACCEPT         receiver -> sender, chosen quality + credit
    0x04 DECLINE        receiver -> sender
    0x05 CHUNK          sender -> receiver, file data
    0x06 CREDIT         receiver -> sender, flow control grant
    0x07 COMPLETE       receiver -> sender, hash verified
    0x08 ERROR          both directions, then close
    0x09 BYE            both directions, orderly shutdown
    0x0A CANCEL         both directions, user-initiated abort

## handshake

Immediately after the transport connects, both peers send HELLO
without waiting for the other:

    HELLO payload:
      magic:    u32     always 0x4B55444F ("KUDO")
      version:  u16     this spec is version 1
      role:     u8      1 = I have a file to send, 2 = I receive
      name_len: u8
      name:     UTF-8   friendly device name, max 64 bytes

If magic is wrong, close silently. If the peer's version is lower,
speak their version if you can, otherwise ERROR code 1 (version
unsupported). If both peers claim the same role, ERROR code 3.

## offer

The sender describes the file:

    OFFER payload:
      file_id:      u32    sender-chosen, echoed in ACCEPT
      total_size:   u64    bytes of the quality level, see below
      chunk_size:   u32    bytes per CHUNK payload, 65536 in v1
      sha256:       32 B   hash of the complete file
      mime_len:     u8
      mime:         UTF-8  e.g. "video/mp4"
      name_len:     u16
      name:         UTF-8  filename
      n_qualities:  u8     always 1 in v1
      per quality:
        quality_id: u8
        bitrate:    u32    bits per second, 0 if unknown
        size:       u64    total bytes at this quality

v1 senders always offer exactly one quality. The receiver picks one
in ACCEPT. The fields exist so v2 adaptive streaming does not need a
new OFFER format.

## accept and credit

    ACCEPT payload:
      file_id:    u32
      quality_id: u8
      credit:     u32    number of chunks the sender may send now

    CREDIT payload:
      file_id:    u32
      credit:     u32    additional chunks granted

Credit is cumulative permission. The sender tracks how many chunks it
has sent and may never exceed the total credit granted. The receiver
grants initial credit in ACCEPT (recommended: enough to fill half its
playback buffer) and tops it up with CREDIT frames as its buffer
drains. A sender that runs out of credit stops sending and waits.

This is the whole flow control story, and later the adaptivity story:
a receiver that keeps granting credit promptly has a healthy buffer.
Starved credit means the link is slower than playback.

## chunks

    CHUNK payload:
      file_id: u32
      index:   u32    zero-based chunk number
      data:    chunk_size bytes, except the final chunk which
               carries total_size mod chunk_size bytes (if nonzero)

Chunks are sent strictly in order, starting at index 0. The transport
is ordered and reliable, so the receiver must treat an out-of-order
index as a protocol error (ERROR code 4): it indicates a bug, not
packet loss.

## completion

After the last chunk, the receiver computes the SHA-256 of the
assembled file and sends COMPLETE with the outcome:

    COMPLETE payload:
      file_id: u32
      status:  u8    0 = hash verified, 1 = hash mismatch

A mismatch is informational, not fatal: by the time it is known, the
content has already been streamed. The receiver decides locally what
to do with the file (discard, keep with warning).

## ending a session

A session ends one of three ways: completion, decline, or error.
COMPLETE and DECLINE are terminal messages: a normal end. ERROR is an
abortive end. The wire encodings are unchanged; this section only fixes
the ordering.

### the close handshake (BYE)

BYE means "I have sent everything I intend to send." The rule:

- After sending a terminal message (COMPLETE or DECLINE), a peer sends
  BYE.
- On receiving BYE, a peer finishes anything it still owes, then sends
  BYE if it has not already.
- A peer closes the transport only once it has both sent AND received
  BYE.

This guarantees neither side closes the socket while the other still
has frames in flight, which is what makes the timing hack unnecessary.
For a file transfer the concrete sequence is:

  1. Receiver finishes receiving, sends COMPLETE, then BYE.
  2. Sender reads COMPLETE (ignoring any surplus CREDIT still arriving),
     reads BYE, sends BYE, closes.
  3. Receiver reads the sender's BYE, closes.

The rule is written direction-agnostically on purpose, so it holds
unchanged when the sender and receiver roles are swapped.

### decline

A receiver may answer an OFFER with DECLINE instead of ACCEPT. DECLINE
is a normal outcome, not an error (the user cancelled, no room for the
file, unacceptable quality, etc.). After DECLINE the receiver runs the
close handshake exactly as for COMPLETE. A sender that receives DECLINE
stops, sends BYE, and closes.

### cancel

Either side may abort a transfer intentionally by sending CANCEL
(file_id) and then closing. A peer that receives CANCEL treats it as a
clean, user-initiated stop: it stops, releases resources (player, temp
files), and exits without error. CANCEL skips the BYE handshake, like
ERROR, but is not a failure. Type byte 0x0A.

### errors are abortive

On a protocol violation a peer sends ERROR and closes immediately,
skipping the BYE handshake. A peer that receives ERROR logs the code
and message and closes. The other end may instead just see the socket
close (EOF); both mean the session aborted.

Codes:
  1 version unsupported   peer's HELLO version cannot be spoken
  2 unknown message type  frame with an unrecognized type byte
  3 role conflict         both peers claimed the same role in HELLO
  4 protocol violation    a valid frame arrived out of sequence
  6 file unavailable      sender cannot read the offered file
  7 internal error        anything else

A hash mismatch is NOT an error. It is reported as COMPLETE with
status = 1, since the data has already been streamed, and the session
still closes cleanly via BYE.

### ERROR is valid at every read

Wherever a peer waits for a specific frame, receiving ERROR is always
an allowed alternative: report it and abort, rather than treating it as
an unexpected-frame violation (which would try to send ERROR back to a
peer that is already closing).

### unexpected disconnect

If a peer reads EOF or hits a transport error anywhere other than the
expected close, it treats the session as aborted: stop, release
resources (player, temp files), close. It does not try to send ERROR,
because the peer is already gone.
