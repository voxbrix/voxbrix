#!/bin/sh

build_scripts_in() {
	# Check if an argument was provided.
	if [ -z "$1" ]; then
		echo "No argument provided."
		return 1
	fi

	# Print the provided argument.
	echo "$1"
	TARGET_DIR="./scripts/server/$1"
	OUTPUT_DIR="./assets/server/scripts/$1"

	# Check if the target is a directory
	if [ ! -d "$TARGET_DIR" ]; then
		echo "Error: $TARGET_DIR is not a directory."
		exit 1
	fi

        # Check if the output is a directory                                 
        if [ ! -d "$OUTPUT_DIR" ]; then                         
                echo "Error: $OUTPUT_DIR is not a directory."   
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
			"../../../../$OUTPUT_DIR/$file_name" \
			&& echo "Finished ${dir%/}"
			# Return to the original directory
			cd - > /dev/null || exit
		fi
	done
}

build_scripts_in server_loop
build_scripts_in chunk_generation
