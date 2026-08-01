package dev.exvynai.kudo

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.EOFException

class FramingTest {

    private fun reader(bytes: ByteArray) = DataInputStream(ByteArrayInputStream(bytes))

    @Test fun readsThreeFramesThenCleanEof() {
        val msgs = listOf(
            Message.Hello(1, Role.SENDER, "takara"),
            Message.Credit(1, 8),
            Message.Bye,
        )
        val buf = ByteArrayOutputStream()
        msgs.forEach { buf.write(it.encode()) }
        val din = reader(buf.toByteArray())

        for (expected in msgs) {
            val got = Framing.readFrame(din)!!
            assertArrayEquals(expected.encode(), got.encode())
        }
        assertNull(Framing.readFrame(din))
    }

    @Test fun oversizedLengthRejected() {
        val din = reader(byteArrayOf(
            0xFF.toByte(), 0xFF.toByte(), 0xFF.toByte(), 0xFF.toByte(), 0x01))
        var threw = false
        try { Framing.readFrame(din) } catch (_: FrameException) { threw = true }
        assertTrue(threw)
    }

    @Test fun truncatedBodyIsError() {
        val din = reader(byteArrayOf(0x00, 0x00, 0x00, 0x09, 0x03, 0x00, 0x00))
        var threw = false
        try { Framing.readFrame(din) } catch (_: EOFException) { threw = true }
        assertTrue(threw)
    }

    @Test fun writeThenReadRoundtrip() {
        val m = Message.Accept(7, 0, 32)
        val buf = ByteArrayOutputStream()
        Framing.writeFrame(buf, m)
        val got = Framing.readFrame(reader(buf.toByteArray()))!!
        assertArrayEquals(m.encode(), got.encode())
    }
}