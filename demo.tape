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

# Launch lo-phi wizard with pre-selected input file
Hide
Type "clear"
Enter
Sleep 500ms
Type "./target/release/lophi -i test_data/wide_test.parquet"
Enter
Sleep 2s
Show
Sleep 2s

# Step 1: Task Selection - "Reduce features" is already highlighted
# Press Enter to select it
Enter
Sleep 1.5s

# Step 2: Target Selection - search for "target" column
Type "target"
Sleep 800ms

# Select the target column
Enter
Sleep 1.5s

# Step 3: Target Mapping - binary target auto-detected, continue
Enter
Sleep 1.5s

# Step 4: Missing Threshold - accept default 0.30
Enter
Sleep 1s

# Step 5: Gini Threshold - accept default 0.05
Enter
Sleep 1s

# Step 6: Correlation Threshold - accept default 0.40
Enter
Sleep 1.5s

# Step 7: Optional Settings - "No - use defaults" is pre-selected
Enter
Sleep 1.5s

# Step 8: Summary - review configuration then run
Sleep 2s
Enter
Sleep 15s

# Show completion results
Sleep 8s
