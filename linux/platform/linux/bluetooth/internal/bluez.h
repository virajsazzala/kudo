/*
 * bluez.h - Linux BlueZ API interface definition
 *
 * Defines the BlueZ API to interface with Linux-based systems.
 */

#ifndef BLUEZ_H
#define BLUEZ_H

#include <dbus/dbus.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include <strings.h>

#define BZ_LOG_ERR(fmt, ...) fprintf(stderr, "[bluez] ERROR: " fmt "\n", ##__VA_ARGS__)
#define BZ_LOG_INFO(fmt, ...) fprintf(stderr, "[bluez] INFO: " fmt "\n", ##__VA_ARGS__)

typedef struct {
	DBusConnection *conn; /* D-Bus connection */
	char path[256];       /* bluetooth adapter path */
} bz_adapter_t;

/**
 * bz_init - connect to system bus and fetch bluetooth adapter
 *
 * @adapter:     caller-allocated adapter struct to fill
 * @hci_address: bluetooth address of the adapter to select (eg: "AA:BB:CC:DD:EE:FF"),
 *               or NULL to select the first adapter found
 *
 * Return: true if connection successful and adapter found, false otherwise.
 */
bool bz_init(bz_adapter_t *adapter, const char *hci_address);

/**
 * bz_powered_on - check bluetooth adapter power state
 *
 * @adapter: adapter returned by bz_init
 *
 * Return: true if enabled, false otherwise.
 */
bool bz_powered_on(bz_adapter_t *adapter);

/**
 * bz_cleanup - close system bus connection
 *
 * @adapter: adapter returned by bz_init
 */
void bz_cleanup(bz_adapter_t *adapter);

#endif /* BLUEZ_H */