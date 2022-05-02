open! Core

val extract_structural_indices
  :  input:Bigstring.t
  -> output:(int64, Bigarray.int64_elt, Bigarray.c_layout) Bigarray.Array1.t
  -> output_index:int
  -> start_offset:int
  -> int

val unescape : input:Bigstring.t -> output:Bigstring.t -> int option

module State : sig
  type t

  val create : direct_emit:(Sexp.t -> unit) -> t

  val process_all
    :  t
    -> Bigstring.t
    -> (int64, Bigarray.int64_elt, Bigarray.c_layout) Bigarray.Array1.t
    -> unit
end

val run : string -> f:(Sexp.t -> unit) -> unit
val of_string_many : string -> Sexp.t list
