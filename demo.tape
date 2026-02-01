# VHS Demo Tape for lo-phi
# Run with: vhs demo.tape

# Output settings
Output demo.gif
Set FontSize 16
Set Width 1200
Set Height 700
Set Theme "Catppuccin Mocha"
Set Padding 20

# Terminal setup
Set Shell "bash"
Set TypingSpeed 80ms



# Launch lo-phi with the test file
Type "./target/release/lophi -i test_data/wide_test.parquet"
Enter
Sleep 2s

# The TUI config menu should now be visible
# Select target column with 'T'
Type "t"
Sleep 1s

# Type to search/filter for target
Type "target"
Sleep 500ms

# Press Enter to select
Enter
Sleep 1s

# Show threshold editing with 'C'
Type "c"
Sleep 800ms

# Clear and type new missing threshold
Ctrl+U
Enter
Sleep 500ms

# Gini threshold
Ctrl+U
Enter
Sleep 500ms

# Correlation threshold
Ctrl+U
Enter
Sleep 1s

# Show the updated config
Sleep 1.5s

# Run the pipeline with Enter
Enter
Sleep 13s

# Show completion
Sleep 2s

# Final pause to show results
Sleep 3s
