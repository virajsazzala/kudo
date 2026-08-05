/*
 * bluetooth.c - Generic bluetooth API implementation
 */

#include "bluetooth/internal/bluez.h"
#include "bluetooth/internal/discovery.h"

#include <kudo/bluetooth/bluetooth.h>

static bz_adapter_t adapter;

bool bt_init(const char *hci_address)
{
	return bz_init(&adapter, hci_address);
}

bool bt_powered_on(void)
{
	return bz_powered_on(&adapter);
}

bool bt_discover(bt_device_t *devices, size_t max_devices, size_t *found)
{
	return bz_discover(&adapter, (bz_device_t *)devices, max_devices, found);
}

void bt_cleanup(void)
{
	bz_cleanup(&adapter);
}