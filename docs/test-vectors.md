# test vectors

## HELLO from a sender named "takara"

frame bytes (hex):
  00 00 00 0F        len = 15
  01                 type = HELLO
  4B 55 44 4F        magic "KUDO"
  00 01              version 1
  01                 role = sender
  06                 name_len = 6
  74 61 6B 61 72 61  "takara"

## CREDIT granting 8 more chunks for file_id 1

  00 00 00 09        len = 9
  06                 type = CREDIT
  00 00 00 01        file_id = 1
  00 00 00 08        credit = 8
