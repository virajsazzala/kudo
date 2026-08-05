/*
 * bluetooth.c - Generic bluetooth API implementation
 */

#include "bluetooth/internal/bluez.h"

#include <kudo/bluetooth/bluetooth.h>

bool bt_init(const char *hci_address)
{
	return bz_init(hci_address);
}

bool bt_powered_on(void)
{
	return bz_powered_on();
}

void bt_cleanup()
{
	bz_cleanup();
}