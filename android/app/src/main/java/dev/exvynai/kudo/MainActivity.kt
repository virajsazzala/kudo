package dev.exvynai.kudo

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.view.View
import android.view.WindowManager
import android.widget.TextView
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import com.google.android.material.button.MaterialButton
import com.google.android.material.progressindicator.LinearProgressIndicator
import kotlinx.coroutines.launch

class MainActivity : AppCompatActivity() {

    private lateinit var fileText: TextView
    private lateinit var statusText: TextView
    private lateinit var progressBar: LinearProgressIndicator
    private lateinit var actionButton: MaterialButton
    private var busy = false

    private val pickVideo = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) startTransfer(uri)
    }
    private val requestPerms = registerForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions()
    ) { /* proceed; a denied BT permission surfaces as a clear error at transfer time */ }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        setContentView(R.layout.activity_main)
        fileText = findViewById(R.id.fileText)
        statusText = findViewById(R.id.statusText)
        progressBar = findViewById(R.id.progressBar)
        actionButton = findViewById(R.id.pickButton)

        actionButton.setOnClickListener {
            if (busy) {
                startService(Intent(this, TransferService::class.java).setAction(TransferService.ACTION_CANCEL))
            } else {
                pickVideo.launch(arrayOf("video/*"))
            }
        }

        requestNeededPermissions()

        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                TransferService.state.collect { render(it) }
            }
        }
    }

    private fun requestNeededPermissions() {
        val perms = mutableListOf<String>()
        if (Build.VERSION.SDK_INT >= 31) perms.add(Manifest.permission.BLUETOOTH_CONNECT)
        if (Build.VERSION.SDK_INT >= 33) perms.add(Manifest.permission.POST_NOTIFICATIONS)
        val missing = perms.filter {
            ContextCompat.checkSelfPermission(this, it) != PackageManager.PERMISSION_GRANTED
        }
        if (missing.isNotEmpty()) requestPerms.launch(missing.toTypedArray())
    }

    private fun startTransfer(uri: Uri) {
        try {
            contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
        } catch (_: Exception) {}
        val intent = Intent(this, TransferService::class.java)
            .setAction(TransferService.ACTION_START)
            .putExtra(TransferService.EXTRA_URI, uri)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        ContextCompat.startForegroundService(this, intent)
    }

    private fun render(s: TransferState) = when (s) {
        is TransferState.Idle -> setUi(false, gone = true, status = "Pick a video to send", button = "Pick a video")
        is TransferState.Waiting -> setUi(true, file = s.fileName, status = "Waiting for laptop to connect…", button = "Cancel")
        is TransferState.Sending -> {
            busy = true
            fileText.visibility = View.VISIBLE; fileText.text = s.fileName
            progressBar.visibility = View.VISIBLE; progressBar.setProgressCompat(s.percent, true)
            statusText.text = "Sending… ${s.percent}%"
            actionButton.text = "Cancel"; actionButton.isEnabled = true
        }
        is TransferState.Done -> setUi(false, gone = true,
            status = if (s.verified) "Sent and verified" else "Sent (hash mismatch)", button = "Send another")
        is TransferState.Error -> setUi(false, gone = true, status = "Error: ${s.message}", button = "Try again")
        is TransferState.Cancelled -> setUi(false, gone = true, status = "Cancelled", button = "Pick a video")
    }

    private fun setUi(busyState: Boolean, file: String? = null, gone: Boolean = false, status: String, button: String) {
        busy = busyState
        if (file != null) { fileText.visibility = View.VISIBLE; fileText.text = file }
        else fileText.visibility = View.GONE
        progressBar.visibility = if (gone) View.GONE else View.GONE
        statusText.text = status
        actionButton.text = button
        actionButton.isEnabled = true
    }
}