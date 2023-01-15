open! Core

module Tape : sig
  type t = (int32, Bigarray.int32_elt, Bigarray.c_layout) Bigarray.Array1.t
  [@@deriving sexp_of]
end

type 'a t [@@deriving sexp_of]

val of_string : string -> [ `single ] t
val of_string_multi : string -> [ `multi ] t

val to_string : [ `single ] t -> string
val to_string_multi : [ `multi ] t -> string

(** Low level visitor API *)

val atom_to_string : [ `atom ] t -> string

val destruct_single
  :  [ `single ] t
  -> [ `Atom of [ `atom ] t | `List of [ `multi ] t]

val visit_single
  :  [ `single ] t
  -> atom:([ `atom ] t -> 'a)
  -> list:([ `multi ] t -> 'a)
  -> 'a

val destruct_multi
  :  [ `multi ] t
  -> ([ `single ] t * [ `multi ] t) option

val visit_multi
  :  [ `multi ] t
  -> nil:(unit -> 'a)
  -> cons:([ `single ] t -> [ `multi ] t -> 'a)
  -> 'a

(** Low level exception API *)

val destruct_atom_exn : [ `single ] t -> [ `atom ] t
val destruct_list_exn : [ `single ] t -> [ `multi ] t

val destruct_nil_exn : [ `multi ] t -> unit
val destruct_cons_exn : [ `multi ] t -> [ `single ] t * [ `multi ] t

(** Multi as container *)

module Multi : sig
  include Container.S0 with type t := [ `multi ] t and type elt := [ `single ] t
end

(** Native conversion *)

val to_native : [ `single ] t -> Sexp.t
val to_native_multi : [ `multi ] t -> Sexp.t list
val of_native : Sexp.t -> [ `single ] t
val of_native_multi : Sexp.t list -> [ `multi ] t

(** Builder API *)

module Builder : sig
  type 'a sexp := 'a t
  type t

  val create : unit -> t
  val append_atom : t -> string -> unit
  val append_list : t -> f:(unit -> unit) -> unit
  val append_list_open : t -> unit
  val append_list_close : t -> unit
  val finalize : t -> [ `multi ] sexp
end

(* TODO: direct to string builder API *)
