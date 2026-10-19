use std::borrow::Cow;

#[derive(Debug)]
pub struct ProtobufSchema {
    inner: prost_types::FileDescriptorSet,
}

impl ProtobufSchema {
    pub fn compile_new(data: Box<[u8]>) -> eyre::Result<Self> {
        struct SingleFileResolver {
            source: String,
        }

        impl protox::file::FileResolver for SingleFileResolver {
            fn open_file(&self, name: &str) -> Result<protox::file::File, protox::Error> {
                if name == "schema.proto" {
                    protox::file::File::from_source(name, &self.source)
                } else {
                    Err(protox::Error::file_not_found(name))
                }
            }
        }

        let data_str = String::from_utf8(data.into_vec())?;
        let file_resolver = SingleFileResolver { source: data_str };

        let mut compiler = protox::Compiler::with_file_resolver(file_resolver);
        compiler.open_file("schema.proto")?;

        Ok(ProtobufSchema {
            inner: compiler.file_descriptor_set(),
        })
    }

    fn inner_file(&self) -> &prost_types::FileDescriptorProto {
        &self.inner.file[0]
    }

    fn protopath_is_absolute(path: &str) -> bool {
        path.starts_with('.')
    }

    fn join_protopath<'a>(root: &str, path: &'a str) -> Cow<'a, str> {
        if Self::protopath_is_absolute(path) {
            Cow::Borrowed(path)
        } else {
            Cow::Owned(format!("{}.{}", root.trim_end_matches('.'), path,))
        }
    }

    fn protopath_parent(path: &str) -> &str {
        if path.is_empty() || path == "." {
            return path;
        }
        let path = path.trim_end_matches('.');
        path.rsplit_once('.').map_or_else(|| ".", |(par, _)| par)
    }

    fn protopath_to_relative<'a>(root: &str, path: &'a str) -> Option<&'a str> {
        let root = root.trim_matches('.');
        let path = path.trim_matches('.');
        path.strip_prefix(root).map(|it| it.trim_start_matches('.'))
    }

    pub fn root_package(&self) -> Option<&str> {
        self.inner_file().package.as_deref()
    }

    pub fn default_message_type(&self) -> Option<Cow<'_, str>> {
        Some(Self::join_protopath(
            self.root_package().unwrap_or_default(),
            self.inner_file().message_type.first().map(|it| it.name())?,
        ))
    }

    pub fn resolve_message_absolute(
        &self,
        path: &str,
    ) -> Option<(String, &prost_types::DescriptorProto)> {
        let path = Self::protopath_to_relative(self.root_package().unwrap_or_default(), path)
            .unwrap_or(path);

        let mut split = path.split(".").filter(|it| !it.is_empty());
        let mut message = {
            let to_find = split.next()?;
            self.inner_file()
                .message_type
                .iter()
                .find(|it| it.name() == to_find)
        };

        for str in split {
            let Some(msg) = message else {
                return None;
            };

            message = msg.nested_type.iter().find(|it| it.name() == str);
        }

        message.map(|m| (path.to_string(), m))
    }

    pub fn resolve_enum_absolute(
        &self,
        path: &str,
    ) -> Option<(String, &prost_types::EnumDescriptorProto)> {
        let path = Self::protopath_to_relative(self.root_package().unwrap_or_default(), path)
            .unwrap_or(path);

        let mut split = path.split(".").filter(|it| !it.is_empty()).peekable();
        let first = split.next()?;
        if split.peek().is_none() {
            return self
                .inner_file()
                .enum_type
                .iter()
                .find(|it| it.name() == first)
                .map(|m| (path.to_string(), m));
        }

        let mut message = {
            self.inner_file()
                .message_type
                .iter()
                .find(|it| it.name() == first)
        };

        while let Some(str) = split.next() {
            if split.peek().is_none() {
                return message.and_then(|m| {
                    m.enum_type
                        .iter()
                        .find(|it| it.name() == str)
                        .map(|m| (path.to_string(), m))
                });
            }

            let Some(msg) = message else {
                return None;
            };

            message = msg.nested_type.iter().find(|it| it.name() == str);
        }

        None
    }

    pub fn resolve_message(
        &self,
        mut from: &str,
        path: &str,
    ) -> Option<(String, &prost_types::DescriptorProto)> {
        loop {
            let full_path = Self::join_protopath(from, path);

            if let Some(value) = self.resolve_message_absolute(&full_path) {
                return Some(value);
            }

            if from.is_empty() || from == "." {
                return None;
            }

            from = Self::protopath_parent(from);
        }
    }

    pub fn resolve_enum(
        &self,
        mut from: &str,
        path: &str,
    ) -> Option<(String, &prost_types::EnumDescriptorProto)> {
        loop {
            let full_path = Self::join_protopath(from, path);

            if let Some(value) = self.resolve_enum_absolute(&full_path) {
                return Some(value);
            }

            if from.is_empty() || from == "." {
                return None;
            }

            from = Self::protopath_parent(from);
        }
    }
}
