/*
 * bluetooth.c - Bluetooth transport implementation
 *
 * Implements device discovery and RFCOMM connection setup over
 * BlueZ's D-Bus API.
 *
 * Responsibilites:
 *     - Power on the local bluetooth adapter.
 *     - Resolve a target device.
 *     - Establish a connected RFCOMM stream to that device
 */