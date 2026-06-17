use crate::hfs::btree::BTreeReader;

pub struct AttributesReader<'a> {
    tree: BTreeReader<'a>,
}

impl<'a> AttributesReader<'a> {
    pub fn open(tree: BTreeReader<'a>) -> Self {
        Self { tree }
    }

    /// Look up a file/directory attribute by its CNID and attribute name.
    pub fn lookup_attribute(&self, _cnid: u32, _name: &str) -> anyhow::Result<Option<Vec<u8>>> {
        log::info!("AttributesReader::lookup_attribute stub");
        Ok(None)
    }
}
