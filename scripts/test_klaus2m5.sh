#!/bin/bash
echo "Running krankulator -s -b input/6502_functional_test.bin, expecting infinite loop on addr 0x3469 ..."
OUTPUT=`./target/release/krankulator -s -b input/6502_functional_test.bin`
#OUTPUT="infite loop detected on addr 0x3469!"
echo $OUTPUT
if [ "infite loop detected on addr 0x3469!" != "${OUTPUT}" ]; then
    exit 1
fi

exit 0
