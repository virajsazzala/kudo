package dev.exvynai.kudo

import java.io.DataInputStream
import java.io.EOFException
import java.io.OutputStream

class FrameException(msg: String) : Exception(msg)

object Framing {
    // Reads one frame. Returns null on clean EOF at a frame boundary (peer
    // closed between frames). EOF partway through a frame throws.
    fun readFrame(input: DataInputStream): Message? {
        val lenSigned = try {
            input.readInt()            // big-endian u32 length prefix
        } catch (e: EOFException) {
            return null                // clean close between frames
        }
        val len = lenSigned.toLong() and 0xFFFFFFFFL   // treat as unsigned
        if (len == 0L) throw FrameException("zero-length frame")
        if (4 + len > MAX_FRAME) throw FrameException("frame too large: $len")
        val body = ByteArray(len.toInt())
        input.readFully(body)          // throws EOFException if truncated
        return Message.decode(body)
    }

    fun writeFrame(output: OutputStream, m: Message) {
        output.write(m.encode())
        output.flush()
    }
}