package dev.exvynai.kudo

sealed class TransferState {
    object Idle : TransferState()
    data class Waiting(val fileName: String) : TransferState()
    data class Sending(val fileName: String, val percent: Int) : TransferState()
    data class Done(val fileName: String, val verified: Boolean) : TransferState()
    data class Error(val message: String) : TransferState()
    object Cancelled : TransferState()
}