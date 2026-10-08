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
  "dmg_sound/rom_singles/01-registers.gb|c6b9fa4b9d9d26919b33ebe78a6ef19ad2df854186cf741ca2746179cc9fc3f1"
  "dmg_sound/rom_singles/02-len ctr.gb|745544125a5065729cab22494a79f65c3836e4426f89e4cc43d330afe711b413"
  "dmg_sound/rom_singles/03-trigger.gb|bb11e7266a7143bafb8aa2a73ca70957c6011f36cd4e0b6aaf0678378294e75c"
  "dmg_sound/rom_singles/04-sweep.gb|58bc14541d91bb020c7761b423825b3432cfde7ee2fa4d1116126e4cb9573c7e"
  "dmg_sound/rom_singles/05-sweep details.gb|f582ca3a0b2544b9510797d7d4dd56a17f53e000511b0ae88a6353200dcd9167"
  "dmg_sound/rom_singles/06-overflow on trigger.gb|1a511e95e84ed6fe6077cabf451a98b4e01fdef59cadae3efbc178113eb064b1"
  "dmg_sound/rom_singles/07-len sweep period sync.gb|56bf5b0c18b996929c9b052ba5a02b450cb619bb6c24cc1d34ea20951c977bf7"
  "dmg_sound/rom_singles/08-len ctr during power.gb|31cb41f7be106a708ec0bc94f2a9d0b506d247cd591e9d0a2a6a960dc3bf6595"
  "dmg_sound/rom_singles/09-wave read while on.gb|378b86f6a25daa16855260d7ef0c24e48146ba05df434a0b5413983a1447e875"
  "dmg_sound/rom_singles/10-wave trigger while on.gb|fa63c8ed7473411e54285d318e33bf23ff6d637ed2caa7555ddeaf80578e3279"
  "dmg_sound/rom_singles/11-regs after power.gb|d27dab46e8b881028723f1975328572d38e25d295289b974cd698916a0be5dab"
  "dmg_sound/rom_singles/12-wave write while on.gb|2efbecd2c6d40928d44f45da4f634626bcd2790165b3949aedfe377c73913774"
  "cgb_sound/rom_singles/01-registers.gb|8415d6c69f7954e9365a586bac235f5ce39270ece32c09cb8ebbbff043b9852c"
  "cgb_sound/rom_singles/02-len ctr.gb|a1ccf47375a2bede3077ac85a4903cb037dd8a4c7c85fc5d0c176af52915271f"
  "cgb_sound/rom_singles/03-trigger.gb|70173be396d14ac955c47aededcee90ba994206cc403273d0bb49ba163ae23ef"
  "cgb_sound/rom_singles/04-sweep.gb|abfd1f6fa9e6701e46135ac9f5bb5b10a46cde22d0fc34922636aa09a5647483"
  "cgb_sound/rom_singles/05-sweep details.gb|f51a80c5538fe4b3afbc5c9af0e689a67869b1e2bbb73d4c60a8874826d056cb"
  "cgb_sound/rom_singles/06-overflow on trigger.gb|5701414d13ba55d25b59d13076e2e9cb59a9da6890595b41464f957605e317c0"
  "cgb_sound/rom_singles/07-len sweep period sync.gb|47d57ba87c3a93050ee340cf49b9d276e62e9573542b92b1de5fa2f17e3716c1"
  "cgb_sound/rom_singles/08-len ctr during power.gb|be7948c70946e0b81c178b8778ef1907231e734650062f8fa8cdb1ceb5ac8c65"
  "cgb_sound/rom_singles/09-wave read while on.gb|9b101c68b2de1930df74c19aaf22cce74d37dd5aade93855c805ecb2e1e83fd3"
  "cgb_sound/rom_singles/10-wave trigger while on.gb|cbea1771a5384f4d8f8a249056224a750ad5ee4671297836e4ba87e689af2a23"
  "cgb_sound/rom_singles/11-regs after power.gb|d347dd195a78c2d0f665b94da493a857218f032b014654b84ddb01f7b8e9befc"
  "cgb_sound/rom_singles/12-wave.gb|7bbd2e78e159de4297f567fec7851f9b90c3076d18d1043af5f30e14bd66cd0a"
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

# dmg-acid2 (MIT): ROM과 기준 스크린샷
ACID2_DEST="$ROOT/tests/roms/dmg-acid2"
ACID2_FILES=(
  "https://github.com/mattcurrie/dmg-acid2/releases/download/v1.0/dmg-acid2.gb|dmg-acid2.gb|464e14b7d42e7feea0b7ede42be7071dc88913f75b9ffa444299424b63d1dff1"
  "https://raw.githubusercontent.com/mattcurrie/dmg-acid2/8a98ce731f96dde032ffb22ec36dc985d78fdb18/img/reference-dmg.png|reference-dmg.png|ca966d50895c7efef05838590d148c2cbfd7fba57dab986f25b35b4da71abb57"
)
mkdir -p "$ACID2_DEST"
for entry in "${ACID2_FILES[@]}"; do
  IFS='|' read -r url name expected <<< "$entry"
  out="$ACID2_DEST/$name"
  if [[ -f "$out" && "$(sha256 "$out")" == "$expected" ]]; then
    continue
  fi
  curl -sfL --retry 3 -o "$out.tmp" "$url"
  actual="$(sha256 "$out.tmp")"
  if [[ "$actual" != "$expected" ]]; then
    rm -f "$out.tmp"
    echo "SHA256 불일치: $name ($actual)" >&2
    exit 1
  fi
  mv "$out.tmp" "$out"
  echo "내려받음: dmg-acid2/$name"
done
echo "dmg-acid2 준비 완료: $ACID2_DEST"

# cgb-acid2 (MIT): ROM과 기준 스크린샷
CGB_ACID2_DEST="$ROOT/tests/roms/cgb-acid2"
CGB_ACID2_FILES=(
  "https://github.com/mattcurrie/cgb-acid2/releases/download/v1.1/cgb-acid2.gbc|cgb-acid2.gbc|197fb0bcec544f0400527fc707e0a94f55435974986e6986b424ace5de81720e"
  "https://raw.githubusercontent.com/mattcurrie/cgb-acid2/04c6ca40cf75b6a93513fe596de4ab797efaff97/img/reference.png|reference-cgb.png|9ea9c262c5383353e77d715d021a0f7c5ccbe438f88082cb225756e50c4fdf01"
)
mkdir -p "$CGB_ACID2_DEST"
for entry in "${CGB_ACID2_FILES[@]}"; do
  IFS='|' read -r url name expected <<< "$entry"
  out="$CGB_ACID2_DEST/$name"
  if [[ -f "$out" && "$(sha256 "$out")" == "$expected" ]]; then
    continue
  fi
  curl -sfL --retry 3 -o "$out.tmp" "$url"
  actual="$(sha256 "$out.tmp")"
  if [[ "$actual" != "$expected" ]]; then
    rm -f "$out.tmp"
    echo "SHA256 불일치: $name ($actual)" >&2
    exit 1
  fi
  mv "$out.tmp" "$out"
  echo "내려받음: cgb-acid2/$name"
done
echo "cgb-acid2 준비 완료: $CGB_ACID2_DEST"

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
