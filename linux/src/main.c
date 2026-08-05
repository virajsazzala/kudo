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

	bt_cleanup();
	return 0;
}