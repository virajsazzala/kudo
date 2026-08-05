/*
 * bluetooth.c - Generic bluetooth API implementation
 */

#include "bluetooth/internal/bluez.h"

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

void bt_cleanup()
{
	bz_cleanup(&adapter);
}