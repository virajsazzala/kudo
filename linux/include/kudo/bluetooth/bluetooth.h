/*
 * bluetooth.h - Generic bluetooth API definitions
 *
 * Defines a generic bluetooth API to interface with different platforms.
 */

#ifndef BLUETOOTH_H
#define BLUETOOTH_H

#include <kudo/bluetooth/discovery.h>
#include <stdbool.h>
#include <stddef.h>

/**
 * bt_init - calls the system specific bluetooth init function
 *
 * @hci_address: bluetooth address of the adapter to select (eg: "AA:BB:CC:DD:EE:FF"),
 *               or NULL to select the first adapter found.

 * TODO: currently wraps bz_init, to be made generic.

 * Return: true if connection successful and adapter found, false otherwise.
 */
bool bt_init(const char *hci_address);

/**
 * bt_powered_on - check bluetooth adapter power state
 *
 * Return: true if enabled, false otherwise.
 */
bool bt_powered_on(void);

/**
 * bt_discover - scan for nearby bluetooth devices
 *
 * @devices:     caller-allocated array to fill
 * @max_devices: capacity of @devices
 * @found:       set to number of devices found
 *
 * TODO: the devices type should be generic and not bz_device_t
 *
 * Return: true on success, false otherwise.
 */
bool bt_discover(bt_device_t *devices, size_t max_devices, size_t *found);

/**
 * bt_cleanup - calls the system specific bluetooth cleanup function
 */
void bt_cleanup(void);

#endif