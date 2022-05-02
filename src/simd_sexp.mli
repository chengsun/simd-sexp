open! Core

val extract_structural_indices
  :  input:string
  -> output:int array
  -> output_index:int
  -> start_offset:int
  -> int

val unescape : input:string -> pos:int -> len:int -> output:bytes -> int option

module State : sig
  type t

  val create : direct_emit:(Sexp.t -> unit) -> t
  val process_all : t -> input:string -> indices:int array -> indices_len:int -> unit
end

val run : string -> f:(Sexp.t -> unit) -> unit
val of_string_many : string -> Sexp.t list
