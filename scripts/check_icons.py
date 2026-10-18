#!/usr/bin/env python3
"""Check Windows icons: frames under 256 px must be BMP/DIB, not PNG.

  check_icons.py assets/bigwave.ico                       # the source .ico
  check_icons.py assets/bigwave.ico --exe A.exe [--match] [--setup B.exe]

--exe/--setup read RT_GROUP_ICON/RT_ICON with pefile (pip install pefile).
--match also requires every image of the .ico to be embedded in --exe byte-for-byte.
--dump DIR writes each PE's first icon group back out as DIR/<name>.ico.
"""
import argparse
import os
import struct
import sys

PNG = b"\x89PNG\r\n\x1a\n"


def ico_frames(data):
    n = struct.unpack_from("<HHH", data)[2]
    out = []
    for i in range(n):
        w, h, _, _, _, bpp, size, off = struct.unpack_from("<BBBBHHII", data, 6 + 16 * i)
        out.append((w or 256, h or 256, bpp, data[off:off + size]))
    return out


def kind(blob):
    if blob[:8] == PNG:
        return "PNG"
    hs, bw, bh, _, bpp, comp = struct.unpack_from("<IiiHHI", blob)
    return f"BMP{bpp}" if hs == 40 and comp == 0 else f"?hdr{hs}"


def report(label, frames):
    print(label)
    bad = []
    for w, h, bpp, blob in frames:
        k = kind(blob)
        print(f"  {w:>3}x{h:<3} {bpp:>2}bpp {k:<6} {len(blob):>7} bytes")
        if w < 256 and k != "BMP32":
            bad.append(f"{w}x{h} is {k}")
    return bad


def pe_icon_groups(path):
    import pefile
    pe = pefile.PE(path, fast_load=True)
    pe.parse_data_directories([pefile.DIRECTORY_ENTRY["IMAGE_DIRECTORY_ENTRY_RESOURCE"]])
    icons, groups = {}, []
    for t in pe.DIRECTORY_ENTRY_RESOURCE.entries:
        if t.id == pefile.RESOURCE_TYPE["RT_ICON"]:
            for e in t.directory.entries:
                d = e.directory.entries[0].data.struct
                icons[e.id] = pe.get_data(d.OffsetToData, d.Size)
        if t.id == pefile.RESOURCE_TYPE["RT_GROUP_ICON"]:
            for e in t.directory.entries:
                d = e.directory.entries[0].data.struct
                groups.append(pe.get_data(d.OffsetToData, d.Size))
    out = []
    for g in groups:
        n = struct.unpack_from("<HHH", g)[2]
        fr = []
        for i in range(n):
            w, h, _, _, _, bpp, _, rid = struct.unpack_from("<BBBBHHIH", g, 6 + 14 * i)
            fr.append((w or 256, h or 256, bpp, icons[rid]))
        out.append(fr)
    return out, set(icons.values())


def write_ico(path, frames):
    hdr = struct.pack("<HHH", 0, 1, len(frames))
    off = 6 + 16 * len(frames)
    body = b""
    for w, h, bpp, blob in frames:
        hdr += struct.pack("<BBBBHHII", w % 256, h % 256, 0, 0, 1, bpp, len(blob), off)
        off += len(blob)
        body += blob
    open(path, "wb").write(hdr + body)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("ico")
    ap.add_argument("--exe")
    ap.add_argument("--setup")
    ap.add_argument("--match", action="store_true")
    ap.add_argument("--dump")
    a = ap.parse_args()
    errors = []
    src = ico_frames(open(a.ico, "rb").read())
    errors += [f"{a.ico}: {e}" for e in report(a.ico, src)]
    for label, path in (("exe", a.exe), ("setup", a.setup)):
        if not path:
            continue
        groups, all_icons = pe_icon_groups(path)
        if not groups:
            errors.append(f"{path}: no RT_GROUP_ICON")
            continue
        errors += [f"{path}: {e}" for e in report(f"{path} (first RT_GROUP_ICON of {len(groups)})", groups[0])]
        if label == "exe" and a.match:
            want = {blob for *_, blob in src}
            if not want <= all_icons:
                errors.append(f"{path}: embedded icon does not match {a.ico}")
        if a.dump:
            os.makedirs(a.dump, exist_ok=True)
            write_ico(os.path.join(a.dump, f"{label}.ico"), groups[0])
    for e in errors:
        print(f"::error::{e}")
    if errors:
        sys.exit(1)
    print("OK: icon frames under 256 px are 32-bit BMP" + (" and match the .ico" if a.match else ""))


if __name__ == "__main__":
    main()
