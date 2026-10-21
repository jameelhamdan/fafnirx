"""Input rules shared by the pages that create devices, sources and mirrors."""

import re

NAME_RE = re.compile(r"^[a-z0-9][a-z0-9._-]{0,62}$")
NAME_RULE = "Name: 1-63 characters, lowercase letters, digits, '.', '_' or '-'"
