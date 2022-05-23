open! Core

module Extract_structural_indices : sig
  type t

  val create : unit -> t

  (** Returns (input_index, indices_len) *)
  val run
    :  t
    -> input:string
    -> input_index:int
    -> indices:(int32, Bigarray.int32_elt, Bigarray.c_layout) Bigarray.Array1.t
    -> indices_index:int
    -> int * int
end

module State : sig
  type t

  val create : unit -> t
  val process_all : t -> input:string -> Sexp.t list
end

type rust_sexp

val of_string_many : string -> Sexp.t list
val of_string_many_rust : string -> Sexp.t list
val of_string_many_rust_sexp : string -> rust_sexp list

module Select : sig
  val multi_select
    :  select_keys:string list
    -> assume_machine_input:bool
    -> output_kind:[ `Values | `Labeled | `Csv ]
    -> threads:bool
    -> unit
end
