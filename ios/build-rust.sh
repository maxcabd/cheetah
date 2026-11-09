#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios

cargo build -p runtime --target aarch64-apple-ios --release
cargo build -p runtime --target aarch64-apple-ios-sim --release
cargo build -p runtime --target x86_64-apple-ios --release

rm -rf ios/Libs/Runtime.xcframework
mkdir -p ios/Libs

xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libruntime.a \
  -library target/aarch64-apple-ios-sim/release/libruntime.a \
  -library target/x86_64-apple-ios/release/libruntime.a \
  -output ios/Libs/Runtime.xcframework

echo "Wrote ios/Libs/Runtime.xcframework"
