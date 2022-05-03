open! Core

module Extract_structural_indices : sig
  type t

  val create : unit -> t

  (** Returns (input_index, indices_len) *)
  val run
    :  t
    -> input:string
    -> input_index:int
    -> indices:int array
    -> indices_index:int
    -> int * int
end

val unescape : input:string -> pos:int -> len:int -> output:bytes -> int option

module State : sig
  type t

  val create : unit -> t
  val process_all : t -> input:string -> Sexp.t list
end

val of_string_many : string -> Sexp.t list
