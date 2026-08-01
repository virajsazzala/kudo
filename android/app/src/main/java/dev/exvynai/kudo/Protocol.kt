package dev.exvynai.kudo

import java.io.ByteArrayOutputStream
import java.nio.charset.StandardCharsets.UTF_8

const val MAGIC: Long = 0x4B55444FL      // "KUDO"
const val VERSION: Int = 1
const val MAX_FRAME: Int = 131_072   // 128 KiB safety cap, above a 64 KiB data chunk + header

enum class Role(val wire: Int) {
    SENDER(1),
    RECEIVER(2);

    companion object {
        fun fromWire(b: Int): Role = when (b) {
            1 -> SENDER
            2 -> RECEIVER
            else -> throw DecodeException("bad role: $b")
        }
    }
}

data class Quality(
    val qualityId: Int,   // u8
    val bitrate: Long,    // u32, bits per second, 0 if unknown
    val size: Long,       // u64, total bytes at this quality
)

class DecodeException(msg: String) : Exception(msg)

sealed class Message {

    data class Hello(val version: Int, val role: Role, val name: String) : Message()

    data class Offer(
        val fileId: Long,        // u32
        val totalSize: Long,     // u64
        val chunkSize: Long,     // u32
        val sha256: ByteArray,   // 32 bytes
        val mime: String,
        val name: String,
        val qualities: List<Quality>,
    ) : Message()

    data class Accept(val fileId: Long, val qualityId: Int, val credit: Long) : Message()
    data class Decline(val fileId: Long) : Message()
    data class Chunk(val fileId: Long, val index: Long, val data: ByteArray) : Message()
    data class Credit(val fileId: Long, val credit: Long) : Message()
    data class Complete(val fileId: Long, val status: Int) : Message()
    data class Error(val code: Int, val msg: String) : Message()
    data class Cancel(val fileId: Long) : Message()
    object Bye : Message()

    // Full frame: len(u32) | type(u8) | payload. len counts type + payload.
    fun encode(): ByteArray {
        val (tag, payload) = encodeBody()
        val out = ByteArrayOutputStream(payload.size + 5)
        val len = (payload.size + 1).toLong()   // +1 for the type byte
        out.putU32(len)
        out.write(tag)
        out.write(payload)
        return out.toByteArray()
    }

    private fun encodeBody(): Pair<Int, ByteArray> {
        val p = ByteArrayOutputStream()
        val tag = when (this) {
            is Hello -> {
                p.putU32(MAGIC)
                p.putU16(version)
                p.write(role.wire)
                val nb = name.toByteArray(UTF_8)
                p.write(nb.size)
                p.write(nb)
                T_HELLO
            }
            is Offer -> {
                p.putU32(fileId)
                p.putU64(totalSize)
                p.putU32(chunkSize)
                p.write(sha256)
                val mb = mime.toByteArray(UTF_8)
                p.write(mb.size)
                p.write(mb)
                val nb = name.toByteArray(UTF_8)
                p.putU16(nb.size)
                p.write(nb)
                p.write(qualities.size)
                for (q in qualities) {
                    p.write(q.qualityId)
                    p.putU32(q.bitrate)
                    p.putU64(q.size)
                }
                T_OFFER
            }
            is Accept -> {
                p.putU32(fileId); p.write(qualityId); p.putU32(credit); T_ACCEPT
            }
            is Decline -> { p.putU32(fileId); T_DECLINE }
            is Chunk -> { p.putU32(fileId); p.putU32(index); p.write(data); T_CHUNK }
            is Credit -> { p.putU32(fileId); p.putU32(credit); T_CREDIT }
            is Complete -> { p.putU32(fileId); p.write(status); T_COMPLETE }
            is Error -> {
                p.putU16(code)
                val mb = msg.toByteArray(UTF_8)
                p.putU16(mb.size)
                p.write(mb)
                T_ERROR
            }
            is Cancel -> { p.putU32(fileId); T_CANCEL }
            is Bye -> T_BYE
        }
        return Pair(tag, p.toByteArray())
    }

    companion object {
        const val T_HELLO = 0x01
        const val T_OFFER = 0x02
        const val T_ACCEPT = 0x03
        const val T_DECLINE = 0x04
        const val T_CHUNK = 0x05
        const val T_CREDIT = 0x06
        const val T_COMPLETE = 0x07
        const val T_ERROR = 0x08
        const val T_CANCEL = 0x0A
        const val T_BYE = 0x09

        // Decode a frame body (type + payload), length prefix already stripped.
        fun decode(body: ByteArray): Message {
            if (body.isEmpty()) throw DecodeException("empty frame body")
            val r = Reader(body)
            val tag = r.u8()
            return when (tag) {
                T_HELLO -> {
                    if (r.u32() != MAGIC) throw DecodeException("bad magic")
                    val version = r.u16()
                    val role = Role.fromWire(r.u8())
                    val name = r.utf8(r.u8())
                    Hello(version, role, name)
                }
                T_OFFER -> {
                    val fileId = r.u32()
                    val totalSize = r.u64()
                    val chunkSize = r.u32()
                    val sha256 = r.bytes(32)
                    val mime = r.utf8(r.u8())
                    val name = r.utf8(r.u16())
                    val nq = r.u8()
                    val qs = ArrayList<Quality>(nq)
                    repeat(nq) { qs.add(Quality(r.u8(), r.u32(), r.u64())) }
                    Offer(fileId, totalSize, chunkSize, sha256, mime, name, qs)
                }
                T_ACCEPT -> Accept(r.u32(), r.u8(), r.u32())
                T_DECLINE -> Decline(r.u32())
                T_CHUNK -> Chunk(r.u32(), r.u32(), r.rest())
                T_CREDIT -> Credit(r.u32(), r.u32())
                T_COMPLETE -> Complete(r.u32(), r.u8())
                T_ERROR -> {
                    val code = r.u16()
                    val msg = r.utf8(r.u16())
                    Error(code, msg)
                }
                T_CANCEL -> Cancel(r.u32())
                T_BYE -> Bye
                else -> throw DecodeException("unknown type: $tag")
            }
        }
    }
}

// Big-endian write helpers on ByteArrayOutputStream.
private fun ByteArrayOutputStream.putU16(v: Int) {
    write((v ushr 8) and 0xFF); write(v and 0xFF)
}
private fun ByteArrayOutputStream.putU32(v: Long) {
    write(((v ushr 24) and 0xFF).toInt())
    write(((v ushr 16) and 0xFF).toInt())
    write(((v ushr 8) and 0xFF).toInt())
    write((v and 0xFF).toInt())
}
private fun ByteArrayOutputStream.putU64(v: Long) {
    for (shift in intArrayOf(56, 48, 40, 32, 24, 16, 8, 0)) {
        write(((v ushr shift) and 0xFF).toInt())
    }
}

// Bounds-checked big-endian reader.
private class Reader(val buf: ByteArray) {
    var pos = 0
    private fun need(n: Int) {
        if (pos + n > buf.size) throw DecodeException("truncated")
    }
    fun u8(): Int { need(1); return buf[pos++].toInt() and 0xFF }
    fun u16(): Int { need(2); return (u8() shl 8) or u8() }
    fun u32(): Long {
        need(4)
        var v = 0L
        repeat(4) { v = (v shl 8) or u8().toLong() }
        return v
    }
    fun u64(): Long {
        need(8)
        var v = 0L
        repeat(8) { v = (v shl 8) or u8().toLong() }
        return v
    }
    fun bytes(n: Int): ByteArray { need(n); val b = buf.copyOfRange(pos, pos + n); pos += n; return b }
    fun utf8(n: Int): String = String(bytes(n), UTF_8)
    fun rest(): ByteArray { val b = buf.copyOfRange(pos, buf.size); pos = buf.size; return b }
}