# Setup Wizard Improvements

Three improvements to the TUI setup wizard.

## 1. Arrow Key Navigation

- Up/Down moves focus between form fields (replaces Tab/Shift+Tab)
- Tab reserved for path autocomplete
- Enter on any field advances to next field, Enter on Start launches

## 2. Path Autocomplete (Source, Destination fields)

- On each keystroke in path fields: read parent directory from current input, filter contents by typed prefix
- Dropdown appears automatically when there are matches, shows filtered entries
- Entries sorted alphabetically, directories suffixed with `/`
- Tab inserts selected item (first if none selected)
- Up/Down when dropdown is open navigates within dropdown, not between form fields
- Esc closes dropdown, returns Up/Down to form navigation
- Dropdown: max 8 visible lines, scrollable, selected item highlighted
- Read errors (no permissions, path doesn't exist) result in empty dropdown

### Filtering logic

Given input `/home/us`:
- Parent directory: `/home/`
- Prefix filter: `us`
- Dropdown shows: entries in `/home/` starting with `us`

Given input `/home/user/`:
- Parent directory: `/home/user/`
- Prefix filter: empty (show all contents)
- Dropdown shows: all entries in `/home/user/`

When user selects a directory from dropdown, append `/` so next Tab continues deeper.

## 3. Live Field Validation

Validated on every keystroke:
- `Size` — `ByteSize::parse()`, red if invalid and non-empty
- `Min size` — `ByteSize::parse()`, red if invalid and non-empty
- `Source` — red if non-empty and path doesn't exist

Not validated during input:
- `Destination` — may not exist yet (created during copy)
- `Extensions`, `Exclude` — any text is valid

Red color applies to field value only, not the label. Empty fields are never red.

## Code Changes

- `app.rs`: add `Dropdown` struct to `SetupForm`, autocomplete filtering/scroll logic in `update_setup`, field validation state
- `tui.rs`: render dropdown overlay below path fields, red styling for invalid fields
- No new modules needed
