open! Core

val extract_structural_indices
  :  input:Bigstring.t
  -> output:(int64, Bigarray.int64_elt, Bigarray.c_layout) Bigarray.Array1.t
  -> output_index:int
  -> start_offset:int
  -> int

module State : sig
  type t

  val create : direct_emit:(Sexp.t -> unit) -> t
  val process : t -> Bigstring.t -> int -> unit
  val process_eof : t -> Bigstring.t -> unit
end

val run : string -> f:(Sexp.t -> unit) -> unit
