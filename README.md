# ununi

Ununi is a utility for Windows that allows you to search for and insert Unicode characters. 

## Installation

Either download a release build or build from source with `cargo`. Running the executable will download the Unicode character reference, build the search index, and prompt to ask if you would like Ununi to start on startup/login for your user account. 

## Usage

By default Ununi is configured to open with the Alt+F1 hotkey. Typing will then search the Unicode standard for characters that match the query. Pressing Enter will copy the currently selected character to the window that was in the foreground when the hotkey was pressed. The arrow keys can be used to select a different character or move the cursor for the query text field. Pressing Escape will cancel the search and close the window, returning you to the previous foreground window.

## Configuration

You can configure the hotkey that Ununi uses and the colors by editing a configuration file (not there by default) in `%APPDATA%\ununi\config.toml`. After changing the configuration you must restart Ununi. A sample configuration with notes is given below, it gives the defaults that would be used if the file does not exist. Any key/table can be left out and the default will be used.

```toml
# configure the hotkey
[hotkey]
# the modifier key; can be one of: alt, ctrl, shift, win
mod = "alt"
# the virtual key code of the second key. See MSDN for details: https://msdn.microsoft.com/en-us/library/windows/desktop/dd375731(v=vs.85).aspx
key = 112 #VK_F1
 
# the colors used to draw the interface
[colors]
# color of text and the box around the query
main = [0.9, 0.9, 0.9]
# color of the cursor and character selection box. This is given an alpha value of 0.8
highlight = [0.9, 0.8, 0.6]
# background color
background = [0.1, 0.1, 0.1]
```

## Technical Notes

Ununi uses the clipboard to get characters into applications. This includes sending them the Ctrl-V paste shortcut. It does not yet restore the clipboard contents, although that can be useful if you want to type the same character multiple times. Windows' Unicode support is not exactly fantastic so this seems to be the best way to go about it. Pressing Ctrl+Enter will send the character one UTF-16 codepoint at a time through WM_CHAR messages, which does work for some applications but is notably very janky.

Pressing Pause/Break while Ununi is open will kill the process. It can be restarted by rerunning the executable although it will again query if you want it to run on startup. Pressing NO does not yet actually change anything in this case. If you do not want it to show the message box, passing the `/S` command line flag will disable it.

The query is fed directly into Tantivy, the query language is documented [here](https://tantivy-search.github.io/tantivy/tantivy/query/struct.QueryParser.html).
