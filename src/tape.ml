open! Core

module Tape = struct
  type t = (int32, Bigarray.int32_elt, Bigarray.c_layout) Bigarray.Array1.t

  let sexp_of_t t =
    let children = ref [] in
    for i = (Bigarray.Array1.dim t) - 1 downto 0 do
      children := (Sexp.Atom (Int32.Hex.to_string t.{i})) :: !children;
    done;
    Sexp.List !children
  ;;
end

external _parse_single : string -> (Tape.t, string) result = "ml_rust_parser_single_tape"

let parse_single string =
  match _parse_single string with
  | Ok tape -> Ok tape
  | Error error_string -> Or_error.error_string error_string
;;

external _unsafe_blit_words : Tape.t -> int -> int -> bytes -> unit = "ml_rust_parser_unsafe_blit_words"

let unsafe_blit_words ~src ~src_pos ~len ~dst = _unsafe_blit_words src src_pos len dst

external output_single_tape : Tape.t -> string = "ml_rust_parser_output_single_tape"

module Pointer = struct
  type t =
    { tape : Tape.t
    ; i : int
    ; end_ : int
    }
  [@@deriving sexp_of]

  let create tape =
    { tape; i = 0; end_ = Bigarray.Array1.dim tape }
  ;;

  let read t = Bigarray.Array1.get t.tape t.i |> Int32.to_int_exn

  let is_empty t = t.i = t.end_

  let sub_tape t = Bigarray.Array1.sub t.tape t.i (t.end_ - t.i)
;;
end

module Segment = struct
  type t =
    { tape : Tape.t
    ; i : int
    ; len : int
    }
  [@@deriving sexp_of]

  let sub_tape ~tag t =
    assert (Int32.equal (t.tape.{t.i - 1}) (Int32.of_int_exn tag));
    Bigarray.Array1.sub t.tape (t.i - 1) (t.len + 1)
  ;;
end

type 'a t =
  | Multi : Pointer.t -> [ `multi ] t
  | Single : Segment.t * int (* tag *) -> [ `single ] t
  | Atom : Segment.t -> [ `atom ] t

let sexp_of_t (type a) _ (t : a t) =
  let module Erased = struct
    type t =
      | Multi of Pointer.t
      | Single of Segment.t * int (* tag *)
      | Atom of Segment.t
    [@@deriving sexp_of]
  end
  in
  let erased : Erased.t =
    match t with
    | Multi pointer -> Multi pointer
    | Single (segment, tag) -> Single (segment, tag)
    | Atom segment -> Atom segment
  in
  Erased.sexp_of_t erased
;;

let of_string_multi string =
  let tape = parse_single string |> ok_exn in
  Multi (Pointer.create tape)
;;

let atom_to_string (Atom { tape; i; len = padded_atom_length_in_words }) =
  let atom = Bytes.create ((padded_atom_length_in_words - 1) * 4) in
  unsafe_blit_words ~src:tape ~src_pos:i ~len:padded_atom_length_in_words ~dst:atom;
  Bytes.to_string atom
;;

let destruct_single (Single (segment, tag)) =
  if (tag land 1 = 0)
  then (
    `Atom (Atom { tape = segment.tape; i = segment.i; len = segment.len }))
  else (
    `List (Multi { tape = segment.tape; i = segment.i; end_ = segment.i + segment.len }))

let visit_single single ~atom ~list =
  match destruct_single single with
  | `Atom x -> atom x
  | `List x -> list x
;;

let destruct_multi (Multi pointer) =
  if Pointer.is_empty pointer
  then None
  else (
    let tag = Pointer.read pointer in
    let length_in_words = tag / 2 in
    let child = Single ({ tape = pointer.tape; i = pointer.i + 1; len = length_in_words }, tag) in
    let rest = Multi { tape = pointer.tape; i = pointer.i + 1 + length_in_words ; end_ = pointer.end_ } in
    Some (child, rest))
;;

let visit_multi multi ~nil ~cons =
  match destruct_multi multi with
  | None -> nil ()
  | Some (child, rest) -> cons child rest
;;

module Multi = struct
  module T = struct
    module Elt = struct
      type nonrec t = [ `single ] t

      let equal
          (Single ({ tape = tape1; i = i1; len = (_len1 : int) }, tag1))
          (Single ({ tape = tape2; i = i2; len = (_len2 : int) }, tag2))
        =
        phys_equal tape1 tape2
        && i1 = i2
        (* [&& len1 = len2] not required as this is encoded in [tag1]/[tag2] *)
        && tag1 = tag2
      ;;
    end

    type nonrec t = [ `multi ] t

    let[@tail_mod_cons] rec fold multi ~init ~f =
      match destruct_multi multi with
      | None -> init
      | Some (child, rest) ->
        let init = f init child in
        fold rest ~init ~f
    ;;

    let iter = `Define_using_fold
    let length = `Define_using_fold
  end

  include T
  include Container.Make0 (T)
end

let destruct_atom_exn single =
  visit_single
    single
    ~atom:Fn.id
    ~list:(fun _ -> raise_s [%sexp "expected atom; got list"])
;;

let destruct_list_exn single =
  visit_single
    single
    ~atom:(fun _ -> raise_s [%sexp "expected list; got atom"])
    ~list:Fn.id
;;

let destruct_nil_exn multi =
  visit_multi
    multi
    ~nil:Fn.id
    ~cons:(fun _ _ -> raise_s [%sexp "expected empty list; got nonempty"])
;;

let destruct_cons_exn multi =
  visit_multi
    multi
    ~nil:(fun () -> raise_s [%sexp "expected nonempty list; got empty"])
    ~cons:(fun child rest -> child, rest)
;;

let of_string string : [ `single ] t =
  let t = of_string_multi string in
  let t, rest = destruct_cons_exn t in
  let () = destruct_nil_exn rest in
  t
;;

let to_string (Single (segment, tag)) =
  output_single_tape (Segment.sub_tape ~tag segment)
;;

let to_string_multi (Multi pointer) =
  output_single_tape (Pointer.sub_tape pointer)
;;

let rec to_native single =
  match destruct_single single with
  | `Atom atom -> Sexp.Atom (atom_to_string atom)
  | `List multi -> Sexp.List (to_native_multi multi)
and to_native_multi multi =
  List.map (Multi.to_list multi) ~f:to_native
;;

module Builder = struct
  (** SingleTapeBuilderBox *)
  type t

  external create : unit -> t = "ml_rust_parser_single_tape_builder_create"
  external append_atom : t -> string -> unit = "ml_rust_parser_single_tape_builder_append_atom"
  external append_list_open : t -> unit = "ml_rust_parser_single_tape_builder_append_list_open"
  external append_list_close : t -> unit = "ml_rust_parser_single_tape_builder_append_list_close"

  let append_list t ~f =
    append_list_open t;
    f ();
    append_list_close t
  ;;

  external _finalize : t -> Tape.t = "ml_rust_parser_single_tape_builder_finalize"

  let rec of_native t = function
    | Sexp.Atom a -> append_atom t a
    | Sexp.List children ->
      append_list t ~f:(fun () -> of_native_multi t children)
  and of_native_multi t children =
      List.iter children ~f:(of_native t)
  ;;

  let finalize t =
    let tape = _finalize t in
    Multi (Pointer.create tape)
  ;;
end

let of_native sexp =
  let builder = Builder.create () in
  Builder.of_native builder sexp;
  let multi = Builder.finalize builder in
  let single, rest = destruct_cons_exn multi in
  let () = destruct_nil_exn rest in
  single
;;

let of_native_multi sexps =
  let builder = Builder.create () in
  Builder.of_native_multi builder sexps;
  Builder.finalize builder
;;

module Parser_state = struct
  type t

  external create : unit -> t = "ml_rust_parser_single_tape_parser_state_create"
end

external _parse_partial : Parser_state.t -> string -> (Tape.t, string) result
  = "ml_rust_parser_single_tape_parse_partial"

external _parse_eof : Parser_state.t -> (Tape.t, string) result
  = "ml_rust_parser_single_tape_parse_eof"

let parse_multi_partial parser_state string =
  match _parse_partial parser_state string with
  | Ok tape -> Ok (Multi (Pointer.create tape))
  | Error error_string -> Or_error.error_string error_string
;;

let parse_multi_eof parser_state =
  match _parse_eof parser_state with
  | Ok tape -> Ok (Multi (Pointer.create tape))
  | Error error_string -> Or_error.error_string error_string
;;
