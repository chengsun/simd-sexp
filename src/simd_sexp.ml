open! Core_kernel

external extract_structural_indices
  :  Bigstring.t
  -> (int64, Bigarray.int64_elt, Bigarray.c_layout) Bigarray.Array1.t
  -> int64
  -> int64
  -> int64
  = "ml_extract_structural_indices"
