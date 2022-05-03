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

  val create : unit -> t

  val process_all
    :  t
    -> input:string
    -> indices:int array
    -> indices_len:int
    -> Sexp.t list
end

val of_string_many : string -> Sexp.t list
