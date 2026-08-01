package dev.exvynai.kudo

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothServerSocket
import android.bluetooth.BluetoothSocket
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.Uri
import android.os.Build
import android.os.IBinder
import android.provider.OpenableColumns
import androidx.core.app.NotificationCompat
import androidx.core.content.IntentCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.io.DataInputStream
import java.io.OutputStream
import java.security.MessageDigest
import java.util.UUID
import kotlin.math.min

class ProtocolException(message: String) : Exception(message)

class TransferService : Service() {

    companion object {
        const val ACTION_START = "dev.exvynai.kudo.START"
        const val ACTION_CANCEL = "dev.exvynai.kudo.CANCEL"
        const val EXTRA_URI = "uri"
        private const val CHANNEL_ID = "kudo_transfer"
        private const val NOTIF_ID = 1
        private val serviceUuid = UUID.fromString("7f5c1e29-4a6b-4c9e-9b3a-2d8f0e6a1c55")

        // Single source of truth the Activity observes. Survives Activity recreation.
        private val _state = MutableStateFlow<TransferState>(TransferState.Idle)
        val state: StateFlow<TransferState> = _state.asStateFlow()
    }

    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())
    private var job: Job? = null
    @Volatile private var serverSocket: BluetoothServerSocket? = null
    @Volatile private var socket: BluetoothSocket? = null
    @Volatile private var cancelled = false

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        val channel = NotificationChannel(
            CHANNEL_ID, "Transfers", NotificationManager.IMPORTANCE_LOW
        ).apply { description = "Ongoing kudo video transfers" }
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_CANCEL -> {
                cancelled = true
                // Give the transfer thread a moment to send CANCEL and close
                // gracefully; if it's blocked on a read, force it open.
                scope.launch { kotlinx.coroutines.delay(400); closeSockets() }
            }
            ACTION_START -> {
                val uri = IntentCompat.getParcelableExtra(intent, EXTRA_URI, Uri::class.java)
                if (uri == null) { stopSelf(); return START_NOT_STICKY }
                val name = queryName(uri)
                cancelled = false
                _state.value = TransferState.Waiting(name)
                startForegroundNotif(name, null)
                job = scope.launch { runTransfer(uri, name) }
            }
        }
        return START_NOT_STICKY
    }

    private fun runTransfer(uri: Uri, name: String) {
        val adapter = getSystemService(BluetoothManager::class.java).adapter
        val server = try {
            adapter.listenUsingInsecureRfcommWithServiceRecord("kudo", serviceUuid)
        } catch (e: SecurityException) {
            fail("Bluetooth permission missing"); return
        }
        serverSocket = server

        val sock = try {
            server.accept()
        } catch (e: Exception) {
            if (cancelled) return
            fail("no connection"); return
        } finally {
            try { server.close() } catch (_: Exception) {}
            serverSocket = null
        }
        socket = sock

        val din = DataInputStream(sock.inputStream)
        val out = sock.outputStream
        var aborted = false

        try {
            Framing.writeFrame(out, Message.Hello(VERSION, Role.SENDER, deviceName()))
            when (val req = Framing.readFrame(din)) {
                is Message.Hello ->
                    if (req.role != Role.RECEIVER) {
                        Framing.writeFrame(out, Message.Error(3, "peer role ${req.role}"))
                        throw ProtocolException("role conflict")
                    }
                is Message.Error -> throw ProtocolException("peer ERROR ${req.code}: ${req.msg}")
                null -> throw ProtocolException("peer gone before HELLO")
                else -> { Framing.writeFrame(out, Message.Error(4, "expected HELLO")); throw ProtocolException("expected HELLO, got $req") }
            }

            val md = MessageDigest.getInstance("SHA-256")
            var totalSize = 0L
            (contentResolver.openInputStream(uri) ?: throw ProtocolException("cannot open $name")).use { fin ->
                val buf = ByteArray(65536)
                while (true) { val n = fin.read(buf); if (n < 0) break; md.update(buf, 0, n); totalSize += n }
            }
            val digest = md.digest()

            Framing.writeFrame(out, Message.Offer(
                fileId = 1L, totalSize = totalSize, chunkSize = 65536L,
                sha256 = digest, mime = "video/mp4", name = name,
                qualities = listOf(Quality(0, 1_200_000L, totalSize)),
            ))

            val accept = when (val resp = Framing.readFrame(din)) {
                is Message.Accept -> resp
                is Message.Decline -> { closeAfterBye(din, out); _state.value = TransferState.Error("declined by laptop"); return }
                is Message.Error -> throw ProtocolException("peer ERROR ${resp.code}: ${resp.msg}")
                null -> throw ProtocolException("peer gone after offer")
                else -> { Framing.writeFrame(out, Message.Error(4, "expected ACCEPT/DECLINE")); throw ProtocolException("expected ACCEPT/DECLINE, got $resp") }
            }

            _state.value = TransferState.Sending(name, 0)
            var credit = accept.credit
            var index = 0L
            var sent = 0L
            var lastPct = -1
            (contentResolver.openInputStream(uri) ?: throw ProtocolException("cannot reopen $name")).use { fin ->
                val buf = ByteArray(65536)
                while (sent < totalSize) {
                    if (cancelled) {
                        try { Framing.writeFrame(out, Message.Cancel(1L)) } catch (_: Exception) {}
                        _state.value = TransferState.Cancelled
                        return
                    }
                    while (credit == 0L) {
                        when (val m = Framing.readFrame(din)) {
                            is Message.Credit -> credit += m.credit
                            is Message.Error -> throw ProtocolException("peer ERROR ${m.code}: ${m.msg}")
                            null -> throw ProtocolException("peer gone while sending")
                            else -> { Framing.writeFrame(out, Message.Error(4, "expected CREDIT")); throw ProtocolException("expected CREDIT, got $m") }
                        }
                    }
                    val want = min(buf.size.toLong(), totalSize - sent).toInt()
                    val n = fin.read(buf, 0, want); if (n < 0) break
                    Framing.writeFrame(out, Message.Chunk(1L, index, buf.copyOfRange(0, n)))
                    sent += n; index++; credit--
                    val pct = (sent * 100 / totalSize).toInt()
                    if (pct != lastPct) {
                        lastPct = pct
                        _state.value = TransferState.Sending(name, pct)
                        updateNotif(name, pct)
                    }
                }
            }

            var status = -1
            loop@ while (true) {
                when (val f = Framing.readFrame(din)) {
                    is Message.Credit -> continue@loop
                    is Message.Complete -> { status = f.status; break@loop }
                    is Message.Error -> throw ProtocolException("peer ERROR ${f.code}: ${f.msg}")
                    null -> throw ProtocolException("peer gone before COMPLETE")
                    else -> { Framing.writeFrame(out, Message.Error(4, "expected COMPLETE")); throw ProtocolException("expected COMPLETE, got $f") }
                }
            }
            closeAfterBye(din, out)
            _state.value = TransferState.Done(name, status == 0)
        } catch (e: Exception) {
            aborted = true
            _state.value = if (cancelled) TransferState.Cancelled
            else TransferState.Error(e.message ?: e.toString())
        } finally {
            // Most abort paths just wrote an ERROR frame before throwing; give it
            // a moment to actually reach the peer before the socket is torn down.
            if (aborted && !cancelled) try { Thread.sleep(200) } catch (_: Exception) {}
            try { sock.close() } catch (_: Exception) {}
            socket = null
            stopForegroundAndSelf()
        }
    }

    private fun fail(msg: String) {
        _state.value = TransferState.Error(msg)
        stopForegroundAndSelf()
    }

    private fun closeAfterBye(din: DataInputStream, out: OutputStream) {
        Framing.writeFrame(out, Message.Bye)
        while (true) {
            val f = Framing.readFrame(din)
            if (f == null || f is Message.Bye) return
            when (f) {
                is Message.Credit, is Message.Complete -> continue
                is Message.Error -> throw ProtocolException("peer ERROR ${f.code}: ${f.msg}")
                else -> return
            }
        }
    }

    private fun closeSockets() {
        try { socket?.close() } catch (_: Exception) {}
        try { serverSocket?.close() } catch (_: Exception) {}
    }

    private fun deviceName(): String =
        (getSystemService(BluetoothManager::class.java).adapter?.name) ?: "phone"

    private fun queryName(uri: Uri): String {
        var name = "video.mp4"
        contentResolver.query(uri, null, null, null, null)?.use { c ->
            val idx = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
            if (idx >= 0 && c.moveToFirst()) name = c.getString(idx)
        }
        return name
    }

    private fun buildNotif(title: String, pct: Int?): Notification {
        val cancelIntent = PendingIntent.getService(
            this, 0,
            Intent(this, TransferService::class.java).setAction(ACTION_CANCEL),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )
        val b = NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("kudo")
            .setContentText(if (pct == null) "Waiting for laptop…" else "$title — $pct%")
            .setSmallIcon(android.R.drawable.stat_sys_upload)
            .setOngoing(true)
            .addAction(android.R.drawable.ic_menu_close_clear_cancel, "Cancel", cancelIntent)
        if (pct != null) b.setProgress(100, pct, false)
        return b.build()
    }

    private fun startForegroundNotif(title: String, pct: Int?) {
        val notif = buildNotif(title, pct)
        if (Build.VERSION.SDK_INT >= 34) {
            startForeground(NOTIF_ID, notif, ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
        } else {
            startForeground(NOTIF_ID, notif)
        }
    }

    private fun updateNotif(title: String, pct: Int) {
        getSystemService(NotificationManager::class.java).notify(NOTIF_ID, buildNotif(title, pct))
    }

    private fun stopForegroundAndSelf() {
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    override fun onDestroy() {
        super.onDestroy()
        scope.cancel()
        closeSockets()
    }
}