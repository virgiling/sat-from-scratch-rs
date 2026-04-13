pub trait InProcessing {
    fn reducing(&self) -> bool;
    fn rephasing(&self) -> bool;
    fn probing(&self) -> bool;
    fn eliminating(&self) -> bool;
}
