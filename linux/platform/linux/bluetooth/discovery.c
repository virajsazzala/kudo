/*
 * discovery.c - BlueZ device discovery implementation
 */

#include "internal/discovery.h"

#include "internal/bluez.h"

#define BZ_DBUS_TIMEOUT_MS 3000
#define BZ_DISPATCH_INTERVAL_MS 100
#define BLUEZ_SERVICE "org.bluez"
#define IFACE_ADAPTER1 "org.bluez.Adapter1"
#define IFACE_DEVICE1 "org.bluez.Device1"
#define IFACE_OBJECT_MGR "org.freedesktop.DBus.ObjectManager"

/*
 * start_discovery - call StartDiscovery on the adapter
 *
 * @adapter: adapter returned by bz_init
 *
 * Return: true if discovery is successful, false otherwise.
 */
static bool start_discovery(bz_adapter_t *adapter)
{
	DBusMessage *msg;
	DBusMessage *reply;
	DBusError err;

	msg = dbus_message_new_method_call(BLUEZ_SERVICE, adapter->path, IFACE_ADAPTER1,
	                                   "StartDiscovery");

	if (!msg) {
		BZ_LOG_ERR("failed to allocate StartDiscovery message");
		return false;
	}

	dbus_error_init(&err);

	reply = dbus_connection_send_with_reply_and_block(adapter->conn, msg, BZ_DBUS_TIMEOUT_MS,
	                                                  &err);

	dbus_message_unref(msg);

	if (dbus_error_is_set(&err)) {
		BZ_LOG_ERR("StartDiscovery failed: %s", err.message);
		dbus_error_free(&err);
		return false;
	}

	if (reply)
		dbus_message_unref(reply);

	return true;
}

/*
 * stop_discovery - call StopDiscovery on the adapter
 *
 * @adapter: adapter returned by bz_init
 */
static void stop_discovery(bz_adapter_t *adapter)
{
	DBusMessage *msg;
	DBusMessage *reply;
	DBusError err;

	msg = dbus_message_new_method_call(BLUEZ_SERVICE, adapter->path, IFACE_ADAPTER1,
	                                   "StopDiscovery");
	if (!msg)
		return;

	dbus_error_init(&err);
	reply = dbus_connection_send_with_reply_and_block(adapter->conn, msg, BZ_DBUS_TIMEOUT_MS,
	                                                  &err);
	dbus_message_unref(msg);

	if (dbus_error_is_set(&err)) {
		BZ_LOG_ERR("StopDiscovery failed: %s", err.message);
		dbus_error_free(&err);
	}

	if (reply)
		dbus_message_unref(reply);
}

/*
 * extract_device - Fetch Address, Name, RSSI from property dict
 *
 * @props_iter: iterator positioned at a{sv} property dict
 * @dev:        output device struct
 */
static void extract_device(DBusMessageIter *props_iter, bz_device_t *dev)
{
	while (dbus_message_iter_get_arg_type(props_iter) == DBUS_TYPE_DICT_ENTRY) {
		DBusMessageIter entry, variant;
		const char *key;

		dbus_message_iter_recurse(props_iter, &entry);
		dbus_message_iter_get_basic(&entry, &key);
		dbus_message_iter_next(&entry);
		dbus_message_iter_recurse(&entry, &variant);

		if (strcmp(key, "Address") == 0) {
			const char *addr;
			dbus_message_iter_get_basic(&variant, &addr);
			strncpy(dev->address, addr, sizeof(dev->address) - 1);

		} else if (strcmp(key, "Name") == 0) {
			const char *name;
			dbus_message_iter_get_basic(&variant, &name);
			strncpy(dev->name, name, sizeof(dev->name) - 1);

		} else if (strcmp(key, "RSSI") == 0) {
			dbus_int16_t rssi;
			dbus_message_iter_get_basic(&variant, &rssi);
			dev->rssi = (short)rssi;
		}

		dbus_message_iter_next(props_iter);
	}
}

/*
 * collect_devices - walk GetManagedObjects reply and extract Device1 entries
 *
 * @adapter:     adapter returned by bz_init
 * @devices:     caller-allocated array to fill
 * @max_devices: capacity of @devices
 *
 * Return: number of devices found.
 */
static size_t collect_devices(bz_adapter_t *adapter, bz_device_t *devices, size_t max_devices)
{
	DBusMessage *msg;
	DBusMessage *reply;
	DBusError err;
	DBusMessageIter iter, objects_iter, obj_entry;
	size_t count = 0;

	msg = dbus_message_new_method_call(BLUEZ_SERVICE, "/", IFACE_OBJECT_MGR,
	                                   "GetManagedObjects");

	if (!msg)
		return 0;

	dbus_error_init(&err);

	reply = dbus_connection_send_with_reply_and_block(adapter->conn, msg, BZ_DBUS_TIMEOUT_MS,
	                                                  &err);

	dbus_message_unref(msg);

	if (dbus_error_is_set(&err)) {
		BZ_LOG_ERR("GetManagedObjects failed: %s", err.message);
		dbus_error_free(&err);
		return 0;
	}

	if (!reply)
		return 0;

	dbus_message_iter_init(reply, &iter);
	dbus_message_iter_recurse(&iter, &objects_iter);

	while (dbus_message_iter_get_arg_type(&objects_iter) == DBUS_TYPE_DICT_ENTRY &&
	       count < max_devices) {
		DBusMessageIter ifaces_iter, iface_entry;

		dbus_message_iter_recurse(&objects_iter, &obj_entry);
		dbus_message_iter_next(&obj_entry); /* skip object path */
		dbus_message_iter_recurse(&obj_entry, &ifaces_iter);

		/* look for Device1 interface */
		while (dbus_message_iter_get_arg_type(&ifaces_iter) == DBUS_TYPE_DICT_ENTRY) {
			const char *iface_name;
			DBusMessageIter props_iter;

			dbus_message_iter_recurse(&ifaces_iter, &iface_entry);
			dbus_message_iter_get_basic(&iface_entry, &iface_name);

			if (strcmp(iface_name, IFACE_DEVICE1) == 0) {
				bz_device_t dev = {0};
				dbus_message_iter_next(&iface_entry);
				dbus_message_iter_recurse(&iface_entry, &props_iter);
				extract_device(&props_iter, &dev);

				if (dev.address[0] != '\0')
					devices[count++] = dev;

				break;
			}

			dbus_message_iter_next(&ifaces_iter);
		}

		dbus_message_iter_next(&objects_iter);
	}

	dbus_message_unref(reply);

	return count;
}

/*
 * set_discovery_filter - configure adapter discovery filter
 *
 * @adapter - adapter returned by bz_init
 *
 * Sets the transport to "auto" to discover both BR/EDR (classic) and
 * BLE devices. Must be called before StartDiscovery.
 *
 * Returns: true on success, false otherwise.
 */
static bool set_discovery_filter(bz_adapter_t *adapter)
{
	DBusMessage *msg;
	DBusMessage *reply;
	DBusError err;
	DBusMessageIter args, dict, entry, variant;
	const char *transport = "auto"; /* both BR/EDR and LE */

	msg = dbus_message_new_method_call(BLUEZ_SERVICE, adapter->path, IFACE_ADAPTER1,
	                                   "SetDiscoveryFilter");

	if (!msg)
		return false;

	dbus_message_iter_init_append(msg, &args);
	dbus_message_iter_open_container(&args, DBUS_TYPE_ARRAY, "{sv}", &dict);

	/* Transport: "auto" = BR/EDR + LE, "bredr" = classic only, "le" = LE only */
	dbus_message_iter_open_container(&dict, DBUS_TYPE_DICT_ENTRY, NULL, &entry);
	dbus_message_iter_append_basic(&entry, DBUS_TYPE_STRING, &(const char *){"Transport"});
	dbus_message_iter_open_container(&entry, DBUS_TYPE_VARIANT, "s", &variant);
	dbus_message_iter_append_basic(&variant, DBUS_TYPE_STRING, &transport);
	dbus_message_iter_close_container(&entry, &variant);
	dbus_message_iter_close_container(&dict, &entry);

	dbus_message_iter_close_container(&args, &dict);

	dbus_error_init(&err);
	reply = dbus_connection_send_with_reply_and_block(adapter->conn, msg, BZ_DBUS_TIMEOUT_MS,
	                                                  &err);
	dbus_message_unref(msg);

	if (dbus_error_is_set(&err)) {
		BZ_LOG_ERR("SetDiscoveryFilter failed: %s", err.message);
		dbus_error_free(&err);
		return false;
	}

	if (reply)
		dbus_message_unref(reply);

	return true;
}

bool bz_discover(bz_adapter_t *adapter, bz_device_t *devices, size_t max_devices, size_t *found)
{
	int elapsed_ms = 0;

	*found = 0;

	if (!set_discovery_filter(adapter))
		return false;

	if (!start_discovery(adapter))
		return false;

	BZ_LOG_INFO("scanning for %d seconds...", BZ_DISCOVERY_TIMEOUT_SEC);

	/* dispatch D-Bus messages while waiting for the scan window */
	while (elapsed_ms < BZ_DISCOVERY_TIMEOUT_SEC * 1000) {
		dbus_connection_read_write_dispatch(adapter->conn, BZ_DISPATCH_INTERVAL_MS);
		elapsed_ms += BZ_DISPATCH_INTERVAL_MS;
	}

	stop_discovery(adapter);

	*found = collect_devices(adapter, devices, max_devices);
	BZ_LOG_INFO("found %zu device(s)", *found);

	return true;
}