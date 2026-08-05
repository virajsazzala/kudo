/*
 * main.c - Kudo entry point
 *
 * Parses CLI arguments and drives the receiver end-to-end.
 *
 * Responsibilities:
 *     - Parse and validate CLI options.
 *     - Wire together the transport and protocol layers.
 *     - Manage the stream session.
 *     - Handle all shutdown cases.
 */

#include <kudo/bluetooth/bluetooth.h>
#include <kudo/bluetooth/discovery.h>
#include <stdio.h>

int main(void)
{
	if (!bt_init(NULL)) {
		fprintf(stderr, "error: could not connect to bluetooth daemon.\n"
		                "is bluetooth enabled on your system?\n");
		return 1;
	}

	if (!bt_powered_on()) {
		fprintf(stderr, "error: bluetooth adapter is off.\n"
		                "please turn on bluetooth and try again.\n");
		bt_cleanup();
		return 1;
	}

	printf("bluetooth ready\n");

	bt_device_t devices[BT_MAX_DEVICES];
	size_t found;

	if (!bt_discover(devices, BT_MAX_DEVICES, &found)) {
		fprintf(stderr, "discovery failed\n");
		bt_cleanup();
		return 1;
	}

	for (size_t i = 0; i < found; i++) {
		printf("[%zu] %s  \"%s\"  (RSSI: %d)\n", i, devices[i].address, devices[i].name,
		       devices[i].rssi);
	}

	bt_cleanup();

	return 0;
}