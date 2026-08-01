package dev.exvynai.kudo

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ProtocolTest {

    // Re-encode after decode. Avoids ByteArray structural-equality pitfalls
    // and still proves decode understood every field.
    private fun roundtrip(m: Message) {
        val once = m.encode()
        val decoded = Message.decode(once.copyOfRange(4, once.size))
        assertArrayEquals(once, decoded.encode())
    }

    @Test fun helloVectorExactBytes() {
        val expected = byteArrayOf(
            0x00, 0x00, 0x00, 0x0F, 0x01,
            0x4B, 0x55, 0x44, 0x4F, 0x00, 0x01, 0x01, 0x06,
            0x74, 0x61, 0x6B, 0x61, 0x72, 0x61,
        )
        assertArrayEquals(expected, Message.Hello(1, Role.SENDER, "takara").encode())
    }

    @Test fun creditVectorExactBytes() {
        val expected = byteArrayOf(
            0x00, 0x00, 0x00, 0x09, 0x06,
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x08,
        )
        assertArrayEquals(expected, Message.Credit(1, 8).encode())
    }

    @Test fun helloRoundtrip() = roundtrip(Message.Hello(1, Role.SENDER, "takara"))

    @Test fun offerRoundtrip() = roundtrip(Message.Offer(
        42, 381_000_000L, 65_536L, ByteArray(32) { 0xAB.toByte() },
        "video/mp4", "big-buck-bunny.mp4",
        listOf(Quality(0, 1_200_000L, 381_000_000L)),
    ))

    @Test fun chunkRoundtrip() = roundtrip(Message.Chunk(1, 5810, byteArrayOf(
        0xDE.toByte(), 0xAD.toByte(), 0xBE.toByte(), 0xEF.toByte(), 0x00, 0x7F)))

    @Test fun completeAndByeRoundtrip() {
        roundtrip(Message.Complete(1, 0))
        roundtrip(Message.Complete(1, 1))
        roundtrip(Message.Bye)
    }

    @Test fun errorRoundtrip() {
        roundtrip(Message.Error(4, ""))
        roundtrip(Message.Error(1, "version unsupported"))
    }

    @Test fun unknownTypeRejected() {
        assertThrowsDecode { Message.decode(byteArrayOf(0xFF.toByte())) }
    }

    @Test fun emptyBodyRejected() {
        assertThrowsDecode { Message.decode(byteArrayOf()) }
    }

    @Test fun truncatedRejected() {
        assertThrowsDecode { Message.decode(byteArrayOf(0x03, 0x00, 0x00, 0x00)) }
    }

    @Test fun badMagicRejected() {
        val f = Message.Hello(1, Role.SENDER, "x").encode()
        f[5] = 0x00
        assertThrowsDecode { Message.decode(f.copyOfRange(4, f.size)) }
    }

    @Test fun cancelVectorExactBytes() {
        val expected = byteArrayOf(0x00, 0x00, 0x00, 0x05, 0x0A, 0x00, 0x00, 0x00, 0x01)
        assertArrayEquals(expected, Message.Cancel(1).encode())
    }

    private fun assertThrowsDecode(block: () -> Unit) {
        try {
            block()
            assertTrue("expected DecodeException", false)
        } catch (_: DecodeException) {
        }
    }
}