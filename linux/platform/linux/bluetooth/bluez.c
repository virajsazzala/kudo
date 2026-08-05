/*
 * bluez.c - Linux BlueZ API interface implementation
 */

#include "internal/bluez.h"

#define BZ_DBUS_TIMEOUT_MS 3000
#define BLUEZ_SERVICE "org.bluez"
#define BLUEZ_ROOT_PATH "/"
#define IFACE_OBJECT_MGR "org.freedesktop.DBus.ObjectManager"
#define IFACE_PROPERTIES "org.freedesktop.DBus.Properties"
#define IFACE_ADAPTER1 "org.bluez.Adapter1"

static DBusConnection *conn;
static char adapter_path[256];

/*
 * path_has_interface - check if an object-manager entry exposes an interface
 *
 * @ifaces_iter: iterator positioned at the {sa{sv}} dict for one object path
 * @iface:       interface name to look for (e.g. "org.bluez.Adapter1")
 *
 * The a{sa{sv}} dict maps interface names to property dicts.
 * We only need to match the key (iface name string), so we skip values.
 *
 * Return: true if @iface is present, false otherwise.
 */
static bool path_has_interface(DBusMessageIter *ifaces_iter, const char *iface)
{
	DBusMessageIter iface_entry;

	/* iterate over each {sa{sv}} entry */
	while (dbus_message_iter_get_arg_type(ifaces_iter) == DBUS_TYPE_DICT_ENTRY) {
		const char *iface_name;

		dbus_message_iter_recurse(ifaces_iter, &iface_entry);
		dbus_message_iter_get_basic(&iface_entry, &iface_name);

		if (strcmp(iface_name, iface) == 0) {
			return true;
		}

		dbus_message_iter_next(ifaces_iter);
	}

	return false;
}

/*
 * get_adapter_address - read the "Address" property from an Adapter1 entry
 *
 * @ifaces_iter: iterator positioned at the {sa{sv}} dict for one object path
 * @buf:         output buffer for the address string
 * @buf_len:     size of @buf
 *
 * Return: true if address was found and copied, false otherwise.
 */
static bool get_adapter_address(DBusMessageIter *ifaces_iter, char *buf, size_t buf_len)
{
	DBusMessageIter iface_entry;
	DBusMessageIter props_iter;
	DBusMessageIter prop_entry;

	while (dbus_message_iter_get_arg_type(ifaces_iter) == DBUS_TYPE_DICT_ENTRY) {
		const char *iface_name;

		dbus_message_iter_recurse(ifaces_iter, &iface_entry);
		dbus_message_iter_get_basic(&iface_entry, &iface_name);

		if (strcmp(iface_name, IFACE_ADAPTER1) == 0) {
			/* step to a{sv} property dict */
			dbus_message_iter_next(&iface_entry);
			dbus_message_iter_recurse(&iface_entry, &props_iter);

			/* iterate over each {sv} property entry */
			while (dbus_message_iter_get_arg_type(&props_iter) ==
			       DBUS_TYPE_DICT_ENTRY) {
				const char *prop_name;
				DBusMessageIter variant;

				dbus_message_iter_recurse(&props_iter, &prop_entry);
				dbus_message_iter_get_basic(&prop_entry, &prop_name);

				if (strcmp(prop_name, "Address") == 0) {
					const char *addr;

					dbus_message_iter_next(&prop_entry);
					dbus_message_iter_recurse(&prop_entry, &variant);
					dbus_message_iter_get_basic(&variant, &addr);
					strncpy(buf, addr, buf_len - 1);
					buf[buf_len - 1] = '\0';
					return true;
				}

				dbus_message_iter_next(&props_iter);
			}
		}

		dbus_message_iter_next(ifaces_iter);
	}

	return false;
}

/*
 * find_adapter - discover a BlueZ adapter via GetManagedObjects
 *
 * @hci_address: bluetooth address to match, or NULL for first found.
 *
 * Calls org.freedesktop.DBus.ObjectManager.GetManagedObjects on org.bluez,
 * walks the returned a{oa{sa{sv}}} dict, and saves the path of the first
 * object exposing org.bluez.Adapter1 (optionally matching @hci_address).
 *
 * Return: true and fills adapter_path on success, false otherwise.
 */
static bool find_adapter(const char *hci_address)
{
	DBusMessage *msg;
	DBusMessage *reply;
	DBusError err;
	DBusMessageIter iter;
	DBusMessageIter objects_iter;
	DBusMessageIter obj_entry;
	bool found = false;

	msg = dbus_message_new_method_call(BLUEZ_SERVICE, BLUEZ_ROOT_PATH, IFACE_OBJECT_MGR,
	                                   "GetManagedObjects");

	if (!msg) {
		BZ_LOG_ERR("failed to allocate GetManagedObjects message");
		return false;
	}

	dbus_error_init(&err);

	reply = dbus_connection_send_with_reply_and_block(conn, msg, BZ_DBUS_TIMEOUT_MS, &err);

	dbus_message_unref(msg);

	if (dbus_error_is_set(&err)) {
		BZ_LOG_ERR("GetManagedObjects failed: %s", err.message);
		dbus_error_free(&err);
		return false;
	}

	if (!reply) {
		BZ_LOG_ERR("GetManagedObjects: no reply received");
		return false;
	}

	/*
	 * reply signature: a{oa{sa{sv}}}
	 *   array of:
	 *     object_path  ->  dict of:
	 *       interface_name  ->  dict of:
	 *         property_name  ->  variant
	 */
	dbus_message_iter_init(reply, &iter);

	if (dbus_message_iter_get_arg_type(&iter) != DBUS_TYPE_ARRAY) {
		BZ_LOG_ERR("GetManagedObjects: unexpected reply signature");
		goto out;
	}

	dbus_message_iter_recurse(&iter, &objects_iter);

	/* walk each {o a{sa{sv}}} entry */
	while (dbus_message_iter_get_arg_type(&objects_iter) == DBUS_TYPE_DICT_ENTRY) {
		const char *obj_path;
		DBusMessageIter ifaces_iter;

		dbus_message_iter_recurse(&objects_iter, &obj_entry);
		dbus_message_iter_get_basic(&obj_entry, &obj_path);

		/* step to the a{sa{sv}} value */
		dbus_message_iter_next(&obj_entry);
		dbus_message_iter_recurse(&obj_entry, &ifaces_iter);

		if (path_has_interface(&ifaces_iter, IFACE_ADAPTER1)) {
			if (hci_address) {
				/* reset iterator, path_has_interface consumed it */
				dbus_message_iter_recurse(&obj_entry, &ifaces_iter);

				char addr_buf[18] = {0};

				if (!get_adapter_address(&ifaces_iter, addr_buf,
				                         sizeof(addr_buf))) {
					BZ_LOG_ERR("adapter at %s: could not read Address",
					           obj_path);
					dbus_message_iter_next(&objects_iter);
					continue;
				}

				if (strcasecmp(addr_buf, hci_address) != 0) {
					BZ_LOG_INFO("skipping adapter %s (address %s)", obj_path,
					            addr_buf);
					dbus_message_iter_next(&objects_iter);
					continue;
				}
			}

			strncpy(adapter_path, obj_path, sizeof(adapter_path) - 1);
			adapter_path[sizeof(adapter_path) - 1] = '\0';
			BZ_LOG_INFO("using adapter %s", adapter_path);
			found = true;
			break;
		}

		dbus_message_iter_next(&objects_iter);
	}

	if (!found) {
		if (hci_address)
			BZ_LOG_ERR("no adapter found with address %s", hci_address);
		else
			BZ_LOG_ERR("no Adapter1 found in GetManagedObjects reply");
	}

out:
	dbus_message_unref(reply);
	return found;
}

bool bz_init(const char *hci_address)
{
	DBusError err;

	dbus_error_init(&err);

	/* Create connection to system bus */
	conn = dbus_bus_get(DBUS_BUS_SYSTEM, &err);

	if (dbus_error_is_set(&err)) {
		BZ_LOG_ERR("failed to connect to system bus: %s", err.message);
		dbus_error_free(&err);
		return false;
	}

	if (!conn) {
		BZ_LOG_ERR("failed to connect to system bus: unknown error");
		return false;
	}

	return find_adapter(hci_address);
}

bool bz_powered_on(void)
{
	DBusMessage *msg;
	DBusMessage *reply;
	DBusMessageIter iter;
	DBusMessageIter variant;
	bool powered = false;

	/* Get bluetooth adapter properties */
	msg = dbus_message_new_method_call(BLUEZ_SERVICE, adapter_path, IFACE_PROPERTIES, "Get");

	if (!msg) {
		BZ_LOG_ERR("failed to allocate Properties.Get message");
		return false;
	}

	const char *interface = IFACE_ADAPTER1;
	const char *property = "Powered";

	/* Create and send  message/request to get power property */
	dbus_message_append_args(msg, DBUS_TYPE_STRING, &interface, DBUS_TYPE_STRING, &property,
	                         DBUS_TYPE_INVALID);

	reply = dbus_connection_send_with_reply_and_block(conn, msg, BZ_DBUS_TIMEOUT_MS, NULL);

	dbus_message_unref(msg);

	if (!reply) {
		BZ_LOG_ERR("Properties.Get(Powered): no reply");
		return false;
	}

	/* Parse reply to get power state */
	dbus_message_iter_init(reply, &iter);
	dbus_message_iter_recurse(&iter, &variant);

	if (dbus_message_iter_get_arg_type(&variant) != DBUS_TYPE_BOOLEAN) {
		BZ_LOG_ERR("Properties.Get(Powered): unexpected type in reply");
		dbus_message_unref(reply);
		return false;
	}

	dbus_message_iter_get_basic(&variant, &powered);

	dbus_message_unref(reply);
	return powered;
}

void bz_cleanup(void)
{
	if (conn) {
		dbus_connection_unref(conn);
		conn = NULL;
	}
}