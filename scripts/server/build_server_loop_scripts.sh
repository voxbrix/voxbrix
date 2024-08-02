#!/bin/sh

TARGET_DIR="./scripts/server/server_loop"

# Check if the target is a directory
if [ ! -d "$TARGET_DIR" ]; then
    echo "Error: $TARGET_DIR is not a directory."
    exit 1
fi

# Loop through each subdirectory in the target directory
for dir in "$TARGET_DIR"/*/; do
    # Check if it is indeed a directory
    if [ -d "$dir" ]; then
        echo "Building: ${dir%/}"
        # Change to the subdirectory
        cd "$dir" || continue
	file_name="$(basename "$dir").wasm"
        # Execute the build command
        cargo +nightly fmt \
	&& cargo build \
		--target=wasm32-unknown-unknown \
		--release \
	&& cp "target/wasm32-unknown-unknown/release/$file_name" \
		"../../../../assets/server/scripts/server_loop/$file_name" \
	&& echo "Finished ${dir%/}"
        # Return to the original directory
        cd - > /dev/null || exit
    fi
done
