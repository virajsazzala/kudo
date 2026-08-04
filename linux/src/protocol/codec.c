/*
 * codec.c - Kudo protocol codec implementation
 * 
 * Translates messages to and from the exact byte layout defined
 * in Kudo protocol specification.
 *
 * Responsibilities:
 *     - Serialize a message into its wire byte representation.
 *     - Parse and validate incoming bytes into a message.
 */