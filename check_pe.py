import pefile # or manual PE parser
import struct

with open(r"F:\Benali\papyrus\target\release\papyrus.exe", "rb") as f:
    data = f.read()

# Parse DOS header
pe_offset = struct.unpack_from("<I", data, 0x3c)[0]
magic = struct.unpack_from("<H", data, pe_offset)[0]
assert magic == 0x4550, "Not a PE"

num_sections = struct.unpack_from("<H", data, pe_offset + 6)[0]
opt_magic = struct.unpack_from("<H", data, pe_offset + 24)[0]
is_64 = (opt_magic == 0x20b)

import_dir_rva = struct.unpack_from("<I", data, pe_offset + (120 if is_64 else 104))[0]
import_dir_size = struct.unpack_from("<I", data, pe_offset + (124 if is_64 else 108))[0]

# Find section containing import_dir_rva
opt_size = struct.unpack_from("<H", data, pe_offset + 20)[0]
sec_start = pe_offset + 24 + opt_size

sections = []
for i in range(num_sections):
    sec = data[sec_start + i*40 : sec_start + (i+1)*40]
    name = sec[:8].rstrip(b'\x00').decode('ascii', errors='ignore')
    vsize, vrva, rsize, roffset = struct.unpack_from("<IIII", sec, 8)
    sections.append((name, vrva, vsize, roffset, rsize))

def rva_to_offset(rva):
    for name, vrva, vsize, roffset, rsize in sections:
        if vrva <= rva < vrva + max(vsize, rsize):
            return roffset + (rva - vrva)
    return None

import_offset = rva_to_offset(import_dir_rva)
dlls = []
if import_offset:
    curr = import_offset
    while True:
        orig_first_thunk, timedate, fwd_chain, name_rva, first_thunk = struct.unpack_from("<IIIII", data, curr)
        if name_rva == 0:
            break
        name_off = rva_to_offset(name_rva)
        if name_off:
            dll_name = data[name_off:].split(b'\x00')[0].decode('ascii', errors='ignore')
            dlls.append(dll_name)
        curr += 20

print("Imported DLLs:", dlls)
