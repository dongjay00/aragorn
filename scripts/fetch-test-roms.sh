#!/usr/bin/env bash
# 테스트 ROM을 tests/roms/에 내려받는다. 고정된 커밋과 SHA256으로 검증한다.
# ROM은 저장소에 커밋하지 않는다 (.gitignore의 /tests/roms/).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# retrio/gb-test-roms: Blargg 테스트 ROM 미러
COMMIT="c240dd7d700e5c0b00a7bbba52b53e4ee67b5f15"
BASE="https://raw.githubusercontent.com/retrio/gb-test-roms/$COMMIT"
DEST="$ROOT/tests/roms/blargg"

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

# 경로|SHA256
FILES=(
  "cpu_instrs/individual/01-special.gb|fe61349cbaee10cc384b50f356e541c90d1bc380185716706b5d8c465a03cf89"
  "cpu_instrs/individual/02-interrupts.gb|fb90b0d2b9501910c49709abda1d8e70f757dc12020ebf8409a7779bbfd12229"
  "cpu_instrs/individual/03-op sp,hl.gb|ca553e606d9b9c86fbd318f1b916c6f0b9df0cf1774825d4361a3fdff2e5a136"
  "cpu_instrs/individual/04-op r,imm.gb|7686aa7a39ef3d2520ec1037371b5f94dc283fbbfd0f5051d1f64d987bdd6671"
  "cpu_instrs/individual/05-op rp.gb|d504adfa0a4c4793436a154f14492f044d38b3c6db9efc44138f3c9ad138b775"
  "cpu_instrs/individual/06-ld r,r.gb|17ada54b0b9c1a33cd5429fce5b765e42392189ca36da96312222ffe309e7ed1"
  "cpu_instrs/individual/07-jr,jp,call,ret,rst.gb|ab31d3daaaa3a98bdbd9395b64f48c1bdaa889aba5b19dd5aaff4ec2a7d228a3"
  "cpu_instrs/individual/08-misc instrs.gb|974a71fe4c67f70f5cc6e98d4dc8c096057ff8a028b7bfa9f7a4330038cf8b7e"
  "cpu_instrs/individual/09-op r,r.gb|b28e1be5cd95f22bd1ecacdd33c6f03e607d68870e31a47b15a0229033d5ba2a"
  "cpu_instrs/individual/10-bit ops.gb|7f5b8e488c6988b5aaba8c2a74529b7c180c55a58449d5ee89d606a07c53514a"
  "cpu_instrs/individual/11-op a,(hl).gb|0ec0cf9fda3f00becaefa476df6fb526c434abd9d4a4beac237c2c2692dac5d3"
  "instr_timing/instr_timing.gb|646067b3d6c79fda810e9c3f1cb7c0efd5abb0a7ac06437c54e65720c15d9925"
  "mem_timing/individual/01-read_timing.gb|52724532c5709e38e947eb429337c124c38bc68f373874435a7460548098b617"
  "mem_timing/individual/02-write_timing.gb|eea92d3f4e95aab5910e0f7080916a3c42a2b8deae1ee5d45d1e3751d648f3f6"
  "mem_timing/individual/03-modify_timing.gb|2e9067c670ff8b45916bf321677ad04a6896d06a057dbcb82ae9f208a1ae9c34"
)

for entry in "${FILES[@]}"; do
  path="${entry%%|*}"
  expected="${entry##*|}"
  out="$DEST/$path"
  if [[ -f "$out" && "$(sha256 "$out")" == "$expected" ]]; then
    continue
  fi
  mkdir -p "$(dirname "$out")"
  curl -sfL --retry 3 -o "$out.tmp" "$BASE/${path// /%20}"
  actual="$(sha256 "$out.tmp")"
  if [[ "$actual" != "$expected" ]]; then
    rm -f "$out.tmp"
    echo "SHA256 불일치: $path ($actual)" >&2
    exit 1
  fi
  mv "$out.tmp" "$out"
  echo "내려받음: $path"
done
echo "Blargg 테스트 ROM 준비 완료: $DEST"

# mooneye-test-suite 공식 배포본 (MIT)
MTS_NAME="mts-20240926-1737-443f6e1"
MTS_URL="https://gekkio.fi/files/mooneye-test-suite/$MTS_NAME/$MTS_NAME.tar.gz"
MTS_SHA256="e5b1ed3d928d879263f5b852e4ba20514550d5bc7559775b140e8df4ab4dd4b3"
MTS_DEST="$ROOT/tests/roms/mooneye"
MTS_MARKER="$MTS_DEST/.source"

if [[ -f "$MTS_MARKER" && "$(cat "$MTS_MARKER")" == "$MTS_SHA256" ]]; then
  echo "mooneye 테스트 ROM 준비 완료: $MTS_DEST"
  exit 0
fi
mkdir -p "$ROOT/tests/roms"
archive="$ROOT/tests/roms/$MTS_NAME.tar.gz"
curl -sfL --retry 3 -o "$archive" "$MTS_URL"
actual="$(sha256 "$archive")"
if [[ "$actual" != "$MTS_SHA256" ]]; then
  rm -f "$archive"
  echo "SHA256 불일치: $MTS_NAME.tar.gz ($actual)" >&2
  exit 1
fi
rm -rf "$MTS_DEST" "$ROOT/tests/roms/$MTS_NAME"
tar -xzf "$archive" -C "$ROOT/tests/roms"
mv "$ROOT/tests/roms/$MTS_NAME" "$MTS_DEST"
rm -f "$archive"
echo "$MTS_SHA256" > "$MTS_MARKER"
echo "mooneye 테스트 ROM 준비 완료: $MTS_DEST"
