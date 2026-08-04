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