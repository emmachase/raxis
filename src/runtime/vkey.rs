use windows::Win32::UI::Input::KeyboardAndMouse::*;

macro_rules! back_to_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident {
        $($(#[$vmeta:meta])* $vname:ident $(= $val:expr)?,)*
    }) => {
        $(#[$meta])*
        $vis enum $name {
            $($(#[$vmeta])* $vname $(= $val)?,)*
        }

        impl std::convert::TryFrom<i32> for $name {
            type Error = ();

            fn try_from(v: i32) -> Result<Self, Self::Error> {
                match v {
                    $(x if x == $name::$vname as i32 => Ok($name::$vname),)*
                    _ => Err(()),
                }
            }
        }
    }
}

back_to_enum! {
    #[derive(Debug)]
    pub enum VKey {
        /// VK_LBUTTON 	0x01 	Left mouse button
        LBUTTON = VK_LBUTTON.0 as isize,
        /// VK_RBUTTON 	0x02 	Right mouse button
        RBUTTON = VK_RBUTTON.0 as isize,
        /// VK_CANCEL 	0x03 	Control-break processing
        CANCEL = VK_CANCEL.0 as isize,
        /// VK_MBUTTON 	0x04 	Middle mouse button
        MBUTTON = VK_MBUTTON.0 as isize,
        /// VK_XBUTTON1 	0x05 	X1 mouse button
        XBUTTON1 = VK_XBUTTON1.0 as isize,
        /// VK_XBUTTON2 	0x06 	X2 mouse button
        XBUTTON2 = VK_XBUTTON2.0 as isize,
        /// VK_BACK 	0x08 	Backspace key
        BACK = VK_BACK.0 as isize,
        /// VK_TAB 	0x09 	Tab key
        TAB = VK_TAB.0 as isize,
        /// VK_CLEAR 	0x0C 	Clear key
        CLEAR = VK_CLEAR.0 as isize,
        /// VK_RETURN 	0x0D 	Enter key
        RETURN = VK_RETURN.0 as isize,
        /// VK_SHIFT 	0x10 	Shift key
        SHIFT = VK_SHIFT.0 as isize,
        /// VK_CONTROL 	0x11 	Ctrl key
        CONTROL = VK_CONTROL.0 as isize,
        /// VK_MENU 	0x12 	Alt key
        MENU = VK_MENU.0 as isize,
        /// VK_PAUSE 	0x13 	Pause key
        PAUSE = VK_PAUSE.0 as isize,
        /// VK_CAPITAL 	0x14 	Caps lock key
        CAPITAL = VK_CAPITAL.0 as isize,
        // /// KANA = VK_KANA.0 as isize,       /// VK_KANA 	0x15 	IME Kana mode
        //
        // /// HANGUL = VK_HANGUL.0 as isize,   /// VK_HANGUL 	0x15 	IME Hangul mode
        //
        // /// IME_ON = VK_IME_ON.0 as isize,   /// VK_IME_ON 	0x16 	IME On
        //
        // /// JUNJA = VK_JUNJA.0 as isize,     /// VK_JUNJA 	0x17 	IME Junja mode
        //
        // /// FINAL = VK_FINAL.0 as isize,     /// VK_FINAL 	0x18 	IME final mode
        //
        // /// HANJA = VK_HANJA.0 as isize,     /// VK_HANJA 	0x19 	IME Hanja mode
        //
        // /// KANJI = VK_KANJI.0 as isize,     /// VK_KANJI 	0x19 	IME Kanji mode
        //
        // /// IME_OFF = VK_IME_OFF.0 as isize, /// VK_IME_OFF 	0x1A 	IME Off
        /// VK_ESCAPE 	0x1B 	Esc key
        ESCAPE = VK_ESCAPE.0 as isize,
        /// VK_CONVERT 	0x1C 	IME convert
        CONVERT = VK_CONVERT.0 as isize,
        /// VK_NONCONVERT 	0x1D 	IME nonconvert
        NONCONVERT = VK_NONCONVERT.0 as isize,
        /// VK_ACCEPT 	0x1E 	IME accept
        ACCEPT = VK_ACCEPT.0 as isize,
        /// VK_MODECHANGE 	0x1F 	IME mode change request
        MODECHANGE = VK_MODECHANGE.0 as isize,
        /// VK_SPACE 	0x20 	Spacebar key
        SPACE = VK_SPACE.0 as isize,
        /// VK_PRIOR 	0x21 	Page up key
        PRIOR = VK_PRIOR.0 as isize,
        /// VK_NEXT 	0x22 	Page down key
        NEXT = VK_NEXT.0 as isize,
        /// VK_END 	0x23 	End key
        END = VK_END.0 as isize,
        /// VK_HOME 	0x24 	Home key
        HOME = VK_HOME.0 as isize,
        /// VK_LEFT 	0x25 	Left arrow key
        LEFT = VK_LEFT.0 as isize,
        /// VK_UP 	0x26 	Up arrow key
        UP = VK_UP.0 as isize,
        /// VK_RIGHT 	0x27 	Right arrow key
        RIGHT = VK_RIGHT.0 as isize,
        /// VK_DOWN 	0x28 	Down arrow key
        DOWN = VK_DOWN.0 as isize,
        /// VK_SELECT 	0x29 	Select key
        SELECT = VK_SELECT.0 as isize,
        /// VK_PRINT 	0x2A 	Print key
        PRINT = VK_PRINT.0 as isize,
        /// VK_EXECUTE 	0x2B 	Execute key
        EXECUTE = VK_EXECUTE.0 as isize,
        /// VK_SNAPSHOT 	0x2C 	Print screen key
        SNAPSHOT = VK_SNAPSHOT.0 as isize,
        /// VK_INSERT 	0x2D 	Insert key
        INSERT = VK_INSERT.0 as isize,
        /// VK_DELETE 	0x2E 	Delete key
        DELETE = VK_DELETE.0 as isize,
        /// VK_HELP 	0x2F 	Help key
        HELP = VK_HELP.0 as isize,
        /// VK_LWIN 	0x5B 	Left Windows logo key
        LWIN = VK_LWIN.0 as isize,
        /// VK_RWIN 	0x5C 	Right Windows logo key
        RWIN = VK_RWIN.0 as isize,
        /// VK_APPS 	0x5D 	Application key
        APPS = VK_APPS.0 as isize,
        /// VK_SLEEP 	0x5F 	Computer Sleep key
        SLEEP = VK_SLEEP.0 as isize,
        /// VK_NUMPAD0 	0x60 	Numeric keypad 0 key
        NUMPAD0 = VK_NUMPAD0.0 as isize,
        /// VK_NUMPAD1 	0x61 	Numeric keypad 1 key
        NUMPAD1 = VK_NUMPAD1.0 as isize,
        /// VK_NUMPAD2 	0x62 	Numeric keypad 2 key
        NUMPAD2 = VK_NUMPAD2.0 as isize,
        /// VK_NUMPAD3 	0x63 	Numeric keypad 3 key
        NUMPAD3 = VK_NUMPAD3.0 as isize,
        /// VK_NUMPAD4 	0x64 	Numeric keypad 4 key
        NUMPAD4 = VK_NUMPAD4.0 as isize,
        /// VK_NUMPAD5 	0x65 	Numeric keypad 5 key
        NUMPAD5 = VK_NUMPAD5.0 as isize,
        /// VK_NUMPAD6 	0x66 	Numeric keypad 6 key
        NUMPAD6 = VK_NUMPAD6.0 as isize,
        /// VK_NUMPAD7 	0x67 	Numeric keypad 7 key
        NUMPAD7 = VK_NUMPAD7.0 as isize,
        /// VK_NUMPAD8 	0x68 	Numeric keypad 8 key
        NUMPAD8 = VK_NUMPAD8.0 as isize,
        /// VK_NUMPAD9 	0x69 	Numeric keypad 9 key
        NUMPAD9 = VK_NUMPAD9.0 as isize,
        /// VK_MULTIPLY 	0x6A 	Multiply key
        MULTIPLY = VK_MULTIPLY.0 as isize,
        /// VK_ADD 	0x6B 	Add key
        ADD = VK_ADD.0 as isize,
        /// VK_SEPARATOR 	0x6C 	Separator key
        SEPARATOR = VK_SEPARATOR.0 as isize,
        /// VK_SUBTRACT 	0x6D 	Subtract key
        SUBTRACT = VK_SUBTRACT.0 as isize,
        /// VK_DECIMAL 	0x6E 	Decimal key
        DECIMAL = VK_DECIMAL.0 as isize,
        /// VK_DIVIDE 	0x6F 	Divide key
        DIVIDE = VK_DIVIDE.0 as isize,
        /// VK_F1 	0x70 	F1 key
        F1 = VK_F1.0 as isize,
        /// VK_F2 	0x71 	F2 key
        F2 = VK_F2.0 as isize,
        /// VK_F3 	0x72 	F3 key
        F3 = VK_F3.0 as isize,
        /// VK_F4 	0x73 	F4 key
        F4 = VK_F4.0 as isize,
        /// VK_F5 	0x74 	F5 key
        F5 = VK_F5.0 as isize,
        /// VK_F6 	0x75 	F6 key
        F6 = VK_F6.0 as isize,
        /// VK_F7 	0x76 	F7 key
        F7 = VK_F7.0 as isize,
        /// VK_F8 	0x77 	F8 key
        F8 = VK_F8.0 as isize,
        /// VK_F9 	0x78 	F9 key
        F9 = VK_F9.0 as isize,
        /// VK_F10 	0x79 	F10 key
        F10 = VK_F10.0 as isize,
        /// VK_F11 	0x7A 	F11 key
        F11 = VK_F11.0 as isize,
        /// VK_F12 	0x7B 	F12 key
        F12 = VK_F12.0 as isize,
        /// VK_F13 	0x7C 	F13 key
        F13 = VK_F13.0 as isize,
        /// VK_F14 	0x7D 	F14 key
        F14 = VK_F14.0 as isize,
        /// VK_F15 	0x7E 	F15 key
        F15 = VK_F15.0 as isize,
        /// VK_F16 	0x7F 	F16 key
        F16 = VK_F16.0 as isize,
        /// VK_F17 	0x80 	F17 key
        F17 = VK_F17.0 as isize,
        /// VK_F18 	0x81 	F18 key
        F18 = VK_F18.0 as isize,
        /// VK_F19 	0x82 	F19 key
        F19 = VK_F19.0 as isize,
        /// VK_F20 	0x83 	F20 key
        F20 = VK_F20.0 as isize,
        /// VK_F21 	0x84 	F21 key
        F21 = VK_F21.0 as isize,
        /// VK_F22 	0x85 	F22 key
        F22 = VK_F22.0 as isize,
        /// VK_F23 	0x86 	F23 key
        F23 = VK_F23.0 as isize,
        /// VK_F24 	0x87 	F24 key
        F24 = VK_F24.0 as isize,
        /// VK_NUMLOCK 	0x90 	Num lock key
        NUMLOCK = VK_NUMLOCK.0 as isize,
        /// VK_SCROLL 	0x91 	Scroll lock key
        SCROLL = VK_SCROLL.0 as isize,
        /// VK_LSHIFT 	0xA0 	Left Shift key
        LSHIFT = VK_LSHIFT.0 as isize,
        /// VK_RSHIFT 	0xA1 	Right Shift key
        RSHIFT = VK_RSHIFT.0 as isize,
        /// VK_LCONTROL 	0xA2 	Left Ctrl key
        LCONTROL = VK_LCONTROL.0 as isize,
        /// VK_RCONTROL 	0xA3 	Right Ctrl key
        RCONTROL = VK_RCONTROL.0 as isize,
        /// VK_LMENU 	0xA4 	Left Alt key
        LMENU = VK_LMENU.0 as isize,
        /// VK_RMENU 	0xA5 	Right Alt key
        RMENU = VK_RMENU.0 as isize,
        /// VK_BROWSER_BACK 	0xA6 	Browser Back key
        BROWSER_BACK = VK_BROWSER_BACK.0 as isize,
        /// VK_BROWSER_FORWARD 	0xA7 	Browser Forward key
        BROWSER_FORWARD = VK_BROWSER_FORWARD.0 as isize,
        /// VK_BROWSER_REFRESH 	0xA8 	Browser Refresh key
        BROWSER_REFRESH = VK_BROWSER_REFRESH.0 as isize,
        /// VK_BROWSER_STOP 	0xA9 	Browser Stop key
        BROWSER_STOP = VK_BROWSER_STOP.0 as isize,
        /// VK_BROWSER_SEARCH 	0xAA 	Browser Search key
        BROWSER_SEARCH = VK_BROWSER_SEARCH.0 as isize,
        /// VK_BROWSER_FAVORITES 	0xAB 	Browser Favorites key
        BROWSER_FAVORITES = VK_BROWSER_FAVORITES.0 as isize,
        /// VK_BROWSER_HOME 	0xAC 	Browser Start and Home key
        BROWSER_HOME = VK_BROWSER_HOME.0 as isize,
        /// VK_VOLUME_MUTE 	0xAD 	Volume Mute key
        VOLUME_MUTE = VK_VOLUME_MUTE.0 as isize,
        /// VK_VOLUME_DOWN 	0xAE 	Volume Down key
        VOLUME_DOWN = VK_VOLUME_DOWN.0 as isize,
        /// VK_VOLUME_UP 	0xAF 	Volume Up key
        VOLUME_UP = VK_VOLUME_UP.0 as isize,
        /// VK_MEDIA_NEXT_TRACK 	0xB0 	Next Track key
        MEDIA_NEXT_TRACK = VK_MEDIA_NEXT_TRACK.0 as isize,
        /// VK_MEDIA_PREV_TRACK 	0xB1 	Previous Track key
        MEDIA_PREV_TRACK = VK_MEDIA_PREV_TRACK.0 as isize,
        /// VK_MEDIA_STOP 	0xB2 	Stop Media key
        MEDIA_STOP = VK_MEDIA_STOP.0 as isize,
        /// VK_MEDIA_PLAY_PAUSE 	0xB3 	Play/Pause Media key
        MEDIA_PLAY_PAUSE = VK_MEDIA_PLAY_PAUSE.0 as isize,
        /// VK_LAUNCH_MAIL 	0xB4 	Start Mail key
        LAUNCH_MAIL = VK_LAUNCH_MAIL.0 as isize,
        /// VK_LAUNCH_MEDIA_SELECT 	0xB5 	Select Media key
        LAUNCH_MEDIA_SELECT = VK_LAUNCH_MEDIA_SELECT.0 as isize,
        /// VK_LAUNCH_APP1 	0xB6 	Start Application 1 key
        LAUNCH_APP1 = VK_LAUNCH_APP1.0 as isize,
        /// VK_LAUNCH_APP2 	0xB7 	Start Application 2 key
        LAUNCH_APP2 = VK_LAUNCH_APP2.0 as isize,
        /// VK_OEM_1 	0xBA 	It can vary by keyboard. For the US ANSI keyboard , the Semiсolon and Colon key
        OEM_1 = VK_OEM_1.0 as isize,
        /// VK_OEM_PLUS 	0xBB 	For any country/region, the Equals and Plus key
        OEM_PLUS = VK_OEM_PLUS.0 as isize,
        /// VK_OEM_COMMA 	0xBC 	For any country/region, the Comma and Less Than key
        OEM_COMMA = VK_OEM_COMMA.0 as isize,
        /// VK_OEM_MINUS 	0xBD 	For any country/region, the Dash and Underscore key
        OEM_MINUS = VK_OEM_MINUS.0 as isize,
        /// VK_OEM_PERIOD 	0xBE 	For any country/region, the Period and Greater Than key
        OEM_PERIOD = VK_OEM_PERIOD.0 as isize,
        /// VK_OEM_2 	0xBF 	It can vary by keyboard. For the US ANSI keyboard, the Forward Slash and Question Mark key
        OEM_2 = VK_OEM_2.0 as isize,
        /// VK_OEM_3 	0xC0 	It can vary by keyboard. For the US ANSI keyboard, the Grave Accent and Tilde key
        OEM_3 = VK_OEM_3.0 as isize,
        /// VK_GAMEPAD_A 	0xC3 	Gamepad A button
        GAMEPAD_A = VK_GAMEPAD_A.0 as isize,
        /// VK_GAMEPAD_B 	0xC4 	Gamepad B button
        GAMEPAD_B = VK_GAMEPAD_B.0 as isize,
        /// VK_GAMEPAD_X 	0xC5 	Gamepad X button
        GAMEPAD_X = VK_GAMEPAD_X.0 as isize,
        /// VK_GAMEPAD_Y 	0xC6 	Gamepad Y button
        GAMEPAD_Y = VK_GAMEPAD_Y.0 as isize,
        /// VK_GAMEPAD_RIGHT_SHOULDER 	0xC7 	Gamepad Right Shoulder button
        GAMEPAD_RIGHT_SHOULDER = VK_GAMEPAD_RIGHT_SHOULDER.0 as isize,
        /// VK_GAMEPAD_LEFT_SHOULDER 	0xC8 	Gamepad Left Shoulder button
        GAMEPAD_LEFT_SHOULDER = VK_GAMEPAD_LEFT_SHOULDER.0 as isize,
        /// VK_GAMEPAD_LEFT_TRIGGER 	0xC9 	Gamepad Left Trigger button
        GAMEPAD_LEFT_TRIGGER = VK_GAMEPAD_LEFT_TRIGGER.0 as isize,
        /// VK_GAMEPAD_RIGHT_TRIGGER 	0xCA 	Gamepad Right Trigger button
        GAMEPAD_RIGHT_TRIGGER = VK_GAMEPAD_RIGHT_TRIGGER.0 as isize,
        /// VK_GAMEPAD_DPAD_UP 	0xCB 	Gamepad D-pad Up button
        GAMEPAD_DPAD_UP = VK_GAMEPAD_DPAD_UP.0 as isize,
        /// VK_GAMEPAD_DPAD_DOWN 	0xCC 	Gamepad D-pad Down button
        GAMEPAD_DPAD_DOWN = VK_GAMEPAD_DPAD_DOWN.0 as isize,
        /// VK_GAMEPAD_DPAD_LEFT 	0xCD 	Gamepad D-pad Left button
        GAMEPAD_DPAD_LEFT = VK_GAMEPAD_DPAD_LEFT.0 as isize,
        /// VK_GAMEPAD_DPAD_RIGHT 	0xCE 	Gamepad D-pad Right button
        GAMEPAD_DPAD_RIGHT = VK_GAMEPAD_DPAD_RIGHT.0 as isize,
        /// VK_GAMEPAD_MENU 	0xCF 	Gamepad Menu/Start button
        GAMEPAD_MENU = VK_GAMEPAD_MENU.0 as isize,
        /// VK_GAMEPAD_VIEW 	0xD0 	Gamepad View/Back button
        GAMEPAD_VIEW = VK_GAMEPAD_VIEW.0 as isize,
        /// VK_GAMEPAD_LEFT_THUMBSTICK_BUTTON 	0xD1 	Gamepad Left Thumbstick button
        GAMEPAD_LEFT_THUMBSTICK_BUTTON = VK_GAMEPAD_LEFT_THUMBSTICK_BUTTON.0 as isize,
        /// VK_GAMEPAD_RIGHT_THUMBSTICK_BUTTON 	0xD2 	Gamepad Right Thumbstick button
        GAMEPAD_RIGHT_THUMBSTICK_BUTTON = VK_GAMEPAD_RIGHT_THUMBSTICK_BUTTON.0 as isize,
        /// VK_GAMEPAD_LEFT_THUMBSTICK_UP 	0xD3 	Gamepad Left Thumbstick up
        GAMEPAD_LEFT_THUMBSTICK_UP = VK_GAMEPAD_LEFT_THUMBSTICK_UP.0 as isize,
        /// VK_GAMEPAD_LEFT_THUMBSTICK_DOWN 	0xD4 	Gamepad Left Thumbstick down
        GAMEPAD_LEFT_THUMBSTICK_DOWN = VK_GAMEPAD_LEFT_THUMBSTICK_DOWN.0 as isize,
        /// VK_GAMEPAD_LEFT_THUMBSTICK_RIGHT 	0xD5 	Gamepad Left Thumbstick right
        GAMEPAD_LEFT_THUMBSTICK_RIGHT = VK_GAMEPAD_LEFT_THUMBSTICK_RIGHT.0 as isize,
        /// VK_GAMEPAD_LEFT_THUMBSTICK_LEFT 	0xD6 	Gamepad Left Thumbstick left
        GAMEPAD_LEFT_THUMBSTICK_LEFT = VK_GAMEPAD_LEFT_THUMBSTICK_LEFT.0 as isize,
        /// VK_GAMEPAD_RIGHT_THUMBSTICK_UP 	0xD7 	Gamepad Right Thumbstick up
        GAMEPAD_RIGHT_THUMBSTICK_UP = VK_GAMEPAD_RIGHT_THUMBSTICK_UP.0 as isize,
        /// VK_GAMEPAD_RIGHT_THUMBSTICK_DOWN 	0xD8 	Gamepad Right Thumbstick down
        GAMEPAD_RIGHT_THUMBSTICK_DOWN = VK_GAMEPAD_RIGHT_THUMBSTICK_DOWN.0 as isize,
        /// VK_GAMEPAD_RIGHT_THUMBSTICK_RIGHT 	0xD9 	Gamepad Right Thumbstick right
        GAMEPAD_RIGHT_THUMBSTICK_RIGHT = VK_GAMEPAD_RIGHT_THUMBSTICK_RIGHT.0 as isize,
        /// VK_GAMEPAD_RIGHT_THUMBSTICK_LEFT 	0xDA 	Gamepad Right Thumbstick left
        GAMEPAD_RIGHT_THUMBSTICK_LEFT = VK_GAMEPAD_RIGHT_THUMBSTICK_LEFT.0 as isize,
        /// VK_OEM_4 	0xDB 	It can vary by keyboard. For the US ANSI keyboard, the Left Brace key
        OEM_4 = VK_OEM_4.0 as isize,
        /// VK_OEM_5 	0xDC 	It can vary by keyboard. For the US ANSI keyboard, the Backslash and Pipe key
        OEM_5 = VK_OEM_5.0 as isize,
        /// VK_OEM_6 	0xDD 	It can vary by keyboard. For the US ANSI keyboard, the Right Brace key
        OEM_6 = VK_OEM_6.0 as isize,
        /// VK_OEM_7 	0xDE 	It can vary by keyboard. For the US ANSI keyboard, the Apostrophe and Double Quotation Mark key
        OEM_7 = VK_OEM_7.0 as isize,
        /// VK_OEM_8 	0xDF 	It can vary by keyboard. For the Canadian CSA keyboard, the Right Ctrl key
        OEM_8 = VK_OEM_8.0 as isize,
        /// VK_OEM_102 	0xE2 	It can vary by keyboard. For the European ISO keyboard, the Backslash and Pipe key
        OEM_102 = VK_OEM_102.0 as isize,
        /// VK_PROCESSKEY 	0xE5 	IME PROCESS key
        PROCESSKEY = VK_PROCESSKEY.0 as isize,
        /// VK_PACKET 	0xE7 	Used to pass Unicode characters as if they were keystrokes. The VK_PACKET key is the low word of a 32-bit Virtual Key value used for non-keyboard input methods. For more information, see Remark in KEYBDINPUT, SendInput, WM_KEYDOWN, and WM_KEYUP
        PACKET = VK_PACKET.0 as isize,
        /// VK_ATTN 	0xF6 	Attn key
        ATTN = VK_ATTN.0 as isize,
        /// VK_CRSEL 	0xF7 	CrSel key
        CRSEL = VK_CRSEL.0 as isize,
        /// VK_EXSEL 	0xF8 	ExSel key
        EXSEL = VK_EXSEL.0 as isize,
        /// VK_EREOF 	0xF9 	Erase EOF key
        EREOF = VK_EREOF.0 as isize,
        /// VK_PLAY 	0xFA 	Play key
        PLAY = VK_PLAY.0 as isize,
        /// VK_ZOOM 	0xFB 	Zoom key
        ZOOM = VK_ZOOM.0 as isize,
        /// VK_PA1 	0xFD 	PA1 key
        PA1 = VK_PA1.0 as isize,
        /// VK_OEM_CLEAR 	0xFE 	Clear key
        OEM_CLEAR = VK_OEM_CLEAR.0 as isize,
        /// 	0 key
        N0 = 0x30,
        /// 	1 key
        N1 = 0x31,
        /// 	2 key
        N2 = 0x32,
        /// 	3 key
        N3 = 0x33,
        /// 	4 key
        N4 = 0x34,
        /// 	5 key
        N5 = 0x35,
        /// 	6 key
        N6 = 0x36,
        /// 	7 key
        N7 = 0x37,
        /// 	8 key
        N8 = 0x38,
        /// 	9 key
        N9 = 0x39,
        /// 	A key
        A = 0x41,
        /// 	B key
        B = 0x42,
        /// 	C key
        C = 0x43,
        /// 	D key
        D = 0x44,
        /// 	E key
        E = 0x45,
        /// 	F key
        F = 0x46,
        /// 	G key
        G = 0x47,
        /// 	H key
        H = 0x48,
        /// 	I key
        I = 0x49,
        /// 	J key
        J = 0x4A,
        /// 	K key
        K = 0x4B,
        /// 	L key
        L = 0x4C,
        /// 	M key
        M = 0x4D,
        /// 	N key
        N = 0x4E,
        /// 	O key
        O = 0x4F,
        /// 	P key
        P = 0x50,
        /// 	Q key
        Q = 0x51,
        /// 	R key
        R = 0x52,
        /// 	S key
        S = 0x53,
        /// 	T key
        T = 0x54,
        /// 	U key
        U = 0x55,
        /// 	V key
        V = 0x56,
        /// 	W key
        W = 0x57,
        /// 	X key
        X = 0x58,
        /// 	Y key
        Y = 0x59,
        /// 	Z key
        Z = 0x5A,
    }
}
