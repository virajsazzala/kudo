/*
 * discovery.h - BlueZ device discovery interface
 */

#ifndef DISCOVERY_H
#define DISCOVERY_H

#include "bluez.h"

#include <stdbool.h>
#include <stddef.h>

#define BZ_DISCOVERY_TIMEOUT_SEC 10
#define BZ_MAX_DEVICES 64

typedef struct {
	char address[18];
	char name[248]; /* max bluetooth device name length */
	short rssi;
} bz_device_t;

/**
 * bz_discover - scan for nearby bluetooth devices
 *
 * @adapter:     adapter returned by bz_init
 * @devices:     caller-allocated array to fill
 * @max_devices: capacity of @devices
 * @found:       set to number of devices found
 *
 * Return: true on success, false otherwise.
 */
bool bz_discover(bz_adapter_t *adapter, bz_device_t *devices, size_t max_devices, size_t *found);

#endif /* DISCOVERY_H */
