//! 单行文本输入组件。

/// 输入值最大字节长度，防止异常输入耗尽内存。
const INPUT_MAX_BYTES: usize = 4096;

/// 单行文本输入字段。
pub struct InputField {
    pub value: String,
    pub(crate) cursor: usize,
    pub label: &'static str,
}

impl InputField {
    pub fn new(label: &'static str) -> Self {
        Self {
            value: String::new(),
            cursor: 0,
            label,
        }
    }

    pub fn with_value(label: &'static str, value: &str) -> Self {
        let cursor = value.len();
        Self {
            value: value.to_string(),
            cursor,
            label,
        }
    }

    pub fn insert(&mut self, c: char) {
        if self.value.len() + c.len_utf8() > INPUT_MAX_BYTES {
            return;
        }
        self.value.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let prev = match self.value[..self.cursor].char_indices().next_back() {
                Some((i, _)) => i,
                None => 0,
            };
            self.value.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.value.len() {
            let next = match self.value[self.cursor..].char_indices().nth(1) {
                Some((i, _)) => self.cursor + i,
                None => self.value.len(),
            };
            self.value.drain(self.cursor..next);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = match self.value[..self.cursor].char_indices().next_back() {
                Some((i, _)) => i,
                None => 0,
            };
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.value.len() {
            self.cursor = match self.value[self.cursor..].char_indices().nth(1) {
                Some((i, _)) => self.cursor + i,
                None => self.value.len(),
            };
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.value.len();
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::InputField;

    #[test]
    fn insert_and_backspace() {
        let mut f = InputField::new("test");
        f.insert('a');
        f.insert('b');
        f.insert('c');
        assert_eq!(f.value, "abc");
        assert_eq!(f.cursor, 3);
        f.backspace();
        assert_eq!(f.value, "ab");
        assert_eq!(f.cursor, 2);
    }

    #[test]
    fn cursor_movement() {
        let mut f = InputField::with_value("test", "hello");
        assert_eq!(f.cursor, 5);
        f.move_left();
        assert_eq!(f.cursor, 4);
        f.move_home();
        assert_eq!(f.cursor, 0);
        f.move_right();
        assert_eq!(f.cursor, 1);
        f.move_end();
        assert_eq!(f.cursor, 5);
    }

    #[test]
    fn delete_at_cursor() {
        let mut f = InputField::with_value("test", "abc");
        f.cursor = 1;
        f.delete();
        assert_eq!(f.value, "ac");
        assert_eq!(f.cursor, 1);
    }

    #[test]
    fn unicode_handling() {
        let mut f = InputField::new("test");
        f.insert('你');
        f.insert('好');
        assert_eq!(f.value, "你好");
        f.move_left();
        assert_eq!(f.cursor, 3); // '你' is 3 bytes
        f.backspace();
        assert_eq!(f.value, "好");
    }
}
