#!/bin/sh
# Differential tests: builds the C API tests against this library and, if a
# reference Lua 5.1.4 build is available (REF_LIB=/path/to/liblua.a), against
# PUC-Rio too, then diffs the outputs. Without REF_LIB the saved reference
# outputs (ref.txt / ref2.txt, produced with PUC-Rio 5.1.4) are used.
set -e
cd "$(dirname "$0")"
LIB=../target/release/liblua51.a
CFLAGS="-O1 -g -I../include"
LIBS="-lm -ldl -lpthread -Wl,-E"
cc -shared -fPIC -o cmod.so cmod.c -I../include
for t in capi_test capi_test2; do
  cc $CFLAGS -o $t $t.c $LIB $LIBS
done
if [ -n "$REF_LIB" ]; then
  cc $CFLAGS -o capi_test_ref capi_test.c "$REF_LIB" -lm -ldl -Wl,-E && ./capi_test_ref > ref.txt 2>&1
  cc $CFLAGS -o capi_test2_ref capi_test2.c "$REF_LIB" -lm -ldl -Wl,-E && ./capi_test2_ref > ref2.txt 2>&1
fi
./capi_test > out.txt 2>&1 || true
./capi_test2 > out2.txt 2>&1 || true
echo "== capi_test diff vs PUC-Rio (expected: 1 table address) =="
diff ref.txt out.txt || true
echo "== capi_test2 diff vs PUC-Rio (expected: 5.1.1 vs 5.1.4 message wording) =="
diff ref2.txt out2.txt || true
