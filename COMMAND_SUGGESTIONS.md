# Command Suggestions Feature for Arka Agent

## Overview
This feature adds command suggestions with real-time filtering for skills when users type "/" in the chat input field.

## Features Implemented

### 1. Backend (Rust Core)
- Added `list_names()` function in `skill.rs` to retrieve only skill names
- Function returns `Vec<String>` for efficient UI display

### 2. JNI Bindings (Android)
- Added `skillsJson()` function in `lib.rs` to expose skill list to Android
- Function returns JSON array of skill names

### 3. Android Repository
- Added `skills()` function in `ArkaRepo.kt` to call native function
- Handles errors gracefully with empty list fallback

### 4. ViewModel
- Added `_skills` StateFlow to hold skill list
- Added `loadSkills()` function to fetch skills from repository
- Integrated with existing coroutine scope for thread safety

### 5. UI Components
- Created `SkillSuggestions` composable with:
  - Clean dropdown design with shadow and rounded corners
  - Header with "Skill suggestions" label
  - Divider for visual separation
  - Scrollable list of skill names
  - Clickable items with ripple effect

### 6. Input Bar Enhancement
- Modified `InputBar` to accept skills parameter
- Added real-time filtering when user types "/"
- Shows suggestions dropdown above input field
- Auto-completes selected skill with trailing space
- Maintains all existing functionality (attachments, send button, etc.)

## How It Works

1. **Initialization**: When ChatScreen loads, it calls `vm.loadSkills()` to fetch skill names
2. **Trigger**: When user types "/" at the beginning of input, suggestions appear
3. **Filtering**: As user continues typing after "/", suggestions are filtered in real-time
4. **Selection**: User can click a suggestion to auto-complete the skill name
5. **Auto-complete**: Selected skill is inserted with a trailing space for convenience

## Code Architecture

```
Arka (Rust Core)
├── skill::list_names() → Vec<String>
└── arka-android::skillsJson() → JString

ArkaRepo (Kotlin)
└── skills() → List<String>

MainViewModel (Kotlin)
├── _skills: StateFlow<List<String>>
└── loadSkills() → Unit

ChatScreen (Compose)
├── SkillSuggestions() → @Composable
└── InputBar() → @Composable
    └── Shows suggestions when input.startsWith("/")
```

## Technical Details

### Real-time Filtering
```kotlin
val filteredSkills = if (showSuggestions) {
    val query = input.drop(1).lowercase()
    skills.filter { it.lowercase().contains(query) }
} else {
    emptyList()
}
```

### UI Positioning
- Suggestions dropdown appears above the input bar
- Uses `Alignment.TopEnd` for right alignment
- `padding(bottom = 60.dp)` to avoid overlapping with input field

### Error Handling
- Repository catches exceptions and returns empty list
- UI gracefully handles empty skill list (no suggestions shown)
- No impact on existing chat functionality

## Testing Considerations

1. **Empty Skills**: Works correctly (no suggestions shown)
2. **Non-"/" Input**: Suggestions not shown
3. **Filtering**: Case-insensitive substring matching
4. **Selection**: Auto-complete works with trailing space
5. **Performance**: Efficient filtering for large skill lists

## Future Enhancements

1. **Keyboard Navigation**: Arrow keys to navigate suggestions
2. **Multi-word Commands**: Support for complex command patterns
3. **Command History**: Recent commands suggestion
4. **Tool Descriptions**: Show skill descriptions in tooltips
5. **Platform Support**: Extend to Windows desktop version

## Files Modified

1. `crates/arka-core/src/skill.rs` - Added `list_names()` function
2. `crates/arka-android/src/lib.rs` - Added `skillsJson()` JNI function
3. `android/app/src/main/java/com/arka/agent/core/Arka.kt` - Added `external fun skillsJson()`
4. `android/app/src/main/java/com/arka/agent/core/ArkaRepo.kt` - Added `skills()` function
5. `android/app/src/main/java/com/arka/agent/MainViewModel.kt` - Added skills state and `loadSkills()`
6. `android/app/src/main/java/com/arka/agent/ui/screens/ChatScreen.kt` - Added UI components and integration

## Compatibility
- Maintains full backward compatibility
- No breaking changes to existing APIs
- Graceful degradation if skills cannot be loaded
