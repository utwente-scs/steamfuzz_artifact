use std::{
    fmt, fs,
    mem::MaybeUninit,
    ops::{BitXor, Deref, Shr},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    usize
};

use anyhow::{Context, Result};
use common::{FxHashSet, config::{
    emulator::ENABLE_HIT_COUNT,
    fuzzer::{COVERAGE_BITMAP_SIZE, CoverageBitmapEntry},
}};
use serde::{Deserialize, Serialize};

use crate::Address;

static mut COVERAGE_BITMAP: MaybeUninit<RawBitmap> = MaybeUninit::uninit();
static INIT_DONE: AtomicBool = AtomicBool::new(false);


pub const HASH_KEY: u64 = 0x517cc1b727220a95;
static mut LAST_LOCATION: u64 = 0;

pub type RawEntry = CoverageBitmapEntry;

#[derive(Clone)]
pub struct RawBitmap {
    pub code_coverage: Vec<RawEntry>,
    pub cur_cov_bitmap: Option<CoverageBitmap>,
}

impl RawBitmap {
    pub fn read_from(path: &Path) -> Result<Self> {
        // TODO: Not used in regular fuzzing, is not working correctly yet
        let raw_data = fs::read(path)
            .with_context(|| format!("Failed to read raw bitmap from {path:?}"))?;
        
        let bitmap_size = COVERAGE_BITMAP_SIZE;
        
        let (prefix, data, postfix) = unsafe { raw_data.align_to::<RawEntry>() };
        assert!(prefix.is_empty());
        assert!(postfix.is_empty());
        
        let data = data.to_vec();
        let (code_coverage, _rest) = data.split_at(bitmap_size);

        assert!(prefix.is_empty());
        assert!(postfix.is_empty());
        
        Ok(RawBitmap {
            code_coverage: code_coverage.to_vec(),
            cur_cov_bitmap: None,
        })
    }

    pub fn write_to(&self, path: &Path) -> Result<()> {
        // TODO: Not used in regular fuzzing, probably not working
        let mut combined: Vec<RawEntry> = Vec::new();
        combined.extend(&self.code_coverage);

        let (prefix, data, postfix) = unsafe { combined.align_to::<u8>() };
        assert!(prefix.is_empty());
        assert!(postfix.is_empty());

        fs::write(path, data).with_context(|| format!("Failed to write bitmap to {path:?}"))
    }

    pub fn index(&self, edge: u64) -> usize {
        edge as usize & (self.code_coverage.len() - 1)
    }

    fn add(&mut self, edge: u64) {
        let index = self.index(edge);
        let entry = unsafe { self.code_coverage.get_unchecked_mut(index) };
        *entry = (*entry).saturating_add(1);
    }

    fn set(&mut self, edge: u64) {
        let index = self.index(edge);
        let entry = unsafe { self.code_coverage.get_unchecked_mut(index) };
        *entry = 1;
    }



    pub fn update_cov_bitmap(&mut self) {
        self.cur_cov_bitmap = Some(
            edge_bitmap(&mut self.code_coverage, self.cur_cov_bitmap.take())
        );
    }

    pub(crate) fn create_snapshot() -> Self {
        assert!(get_coverage_bitmap().cur_cov_bitmap.is_none());
        get_coverage_bitmap().clone()
    }

    pub(crate) fn restore_snapshot(&self) {
        let bitmap = get_coverage_bitmap_mut();
        bitmap.code_coverage.copy_from_slice(&self.code_coverage);
        bitmap.cur_cov_bitmap.clone_from(&self.cur_cov_bitmap);
    }
}

impl AsRef<[RawEntry]> for RawBitmap {
    fn as_ref(&self) -> &[RawEntry] {
        &self.code_coverage
    }
}

impl AsMut<[RawEntry]> for RawBitmap {
    fn as_mut(&mut self) -> &mut [RawEntry] {
        &mut self.code_coverage
    }
}

impl fmt::Debug for RawBitmap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Rawbitmap {:?}", self.code_coverage)
        // write!(f, "RawBitmap([RawEntry; {}])", self.code_coverage.len())
    }
}

pub(crate) fn init_done() -> bool {
    INIT_DONE.load(Ordering::SeqCst)
}

pub fn init_coverage_bitmap() {
    let bitmap = RawBitmap {
        code_coverage: vec![0; COVERAGE_BITMAP_SIZE],
        cur_cov_bitmap: None,
    };
    unsafe {
        COVERAGE_BITMAP.write(bitmap);
    }
    INIT_DONE.store(true, Ordering::SeqCst);
}


pub fn get_coverage_bitmap() -> &'static RawBitmap {
    debug_assert!(init_done());

    unsafe { COVERAGE_BITMAP.assume_init_ref() }
}

pub fn get_coverage_bitmap_mut() -> &'static mut RawBitmap {
    debug_assert!(init_done());

    unsafe { COVERAGE_BITMAP.assume_init_mut() }
}

pub fn get_last_location() -> u64 {
    let last_location = unsafe { LAST_LOCATION };
    log::trace!("get_last_location() = {:#x?})", last_location);

    last_location
}

pub fn set_last_location(last_location: u64) {
    log::trace!("set_last_location(last_location = {:#x?})", last_location);

    unsafe {
        LAST_LOCATION = last_location;
    }
}

pub fn add_basic_block(pc: u64) {
    // calculate edge
    let current_location = pc.wrapping_mul(HASH_KEY);
    let edge = current_location.bitxor(unsafe { LAST_LOCATION });

    // update coverage bitmap
    if ENABLE_HIT_COUNT {
        get_coverage_bitmap_mut().add(edge);
    } else {
        get_coverage_bitmap_mut().set(edge);
    }

    // update last lcoation
    unsafe { LAST_LOCATION = current_location.rotate_left(5) }
}



// The was originally in fuzzer/coverage.rs

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Edge(usize);

impl Edge {
    pub(crate) fn new(index: usize) -> Self {
        Self(index)
    }

    pub fn from_index(bitmap: &RawBitmap, index: usize) -> Self {
        debug_assert!(index < bitmap.as_ref().len());
        Self::new(index)
    }

    pub fn from_locations(bitmap: &RawBitmap, last: Address, current: Address) -> Self {
        let last_location = (last as u64).wrapping_mul(HASH_KEY).rotate_left(5);
        let current_location = (current as u64).wrapping_mul(HASH_KEY);
        let edge = current_location.bitxor(last_location);
        let index = bitmap.index(edge);

        Self(index)
    }
}

impl Deref for Edge {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}



#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub struct Feature(usize);

const HIT_BUCKET_BITS: usize = hit_bucket_bits();

const fn hit_bucket_bits() -> usize {
    let mut i = 0;

    assert!(RawEntry::BITS < u8::MAX as u32);
    while RawEntry::BITS >= (1 << i) {
        i += 1;
    }

    i
}

impl Feature {
    pub(crate) fn new(index: usize, hit_bucket: u8) -> Self {
        debug_assert!(index <= (usize::MAX >> HIT_BUCKET_BITS));
        Self(index << HIT_BUCKET_BITS | hit_bucket as usize)
    }

    pub fn edge(self) -> Edge {
        Edge::new(self.0 >> HIT_BUCKET_BITS)
    }

    pub fn hit_bucket(self) -> u8 {
        (self.0 & 0xf) as u8
    }

    pub fn as_raw(self) -> usize {
        self.0
    }
}


#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct CoverageBitmap(FxHashSet<Feature>);
// pub struct CoverageBitmap(Vec<Feature>);

impl CoverageBitmap {
    pub fn features(&self) -> &FxHashSet<Feature> {
        &self.0
    }

    pub fn insert(&mut self, f: Feature) -> () {
        self.0.insert(f);
    }
    // pub fn features(&self) -> &[Feature] {
    //     &self.0
    // }
}

pub fn edge_bitmap(raw_bitmap: &mut Vec<RawEntry>, cur_cov_bitmap: Option<CoverageBitmap>) -> CoverageBitmap {
    let raw_bitmap: &mut [RawEntry] = raw_bitmap.as_mut();
    
    // let mut features = vec![Feature(0); raw_bitmap.len()];
    
    // if there was a previous input window, we want to add to it
    let mut features =  if let Some(cur_cov_bitmap) = cur_cov_bitmap {
        cur_cov_bitmap
    } else {
        CoverageBitmap::default()
    };

    let (prefix, qwords, postfix) = unsafe { raw_bitmap.align_to_mut::<usize>() };
    debug_assert!(prefix.is_empty());
    debug_assert!(postfix.is_empty());

    const USIZE_BYTES: usize = (usize::BITS / RawEntry::BITS) as usize;
    const BYTE_MASK: usize = (RawEntry::MAX as usize) << (usize::BITS - RawEntry::BITS);
    const HIT_BUCKET_MASK: usize = (RawEntry::BITS - 1) as usize;

    // let mut feature_idx = 0;
    for (qidx, qword_ptr) in qwords.iter_mut().enumerate() {
        let mut qword = *qword_ptr;

        while qword != 0 {
            let zeros = qword.leading_zeros() as usize;
            let first_bit = zeros & !HIT_BUCKET_MASK;
            let index = (qidx * USIZE_BYTES) + USIZE_BYTES - 1 - first_bit.shr(3);
            let hit_bucket = (RawEntry::BITS as usize) - (zeros & HIT_BUCKET_MASK);

            // unsafe {
                // *features.get_unchecked_mut(feature_idx) = Feature::new(index, hit_bucket as u8)
            features.insert(Feature::new(index, hit_bucket as u8));
            // };
            // feature_idx += 1;

            qword &= (!BYTE_MASK).rotate_right(first_bit as u32);
        }
        *qword_ptr = 0;
    }


    // log::info!("len beofre truncate {}", features.len());
    // features.truncate(feature_idx);
    // log::info!("len after truncate {}", features.len());

    features
}