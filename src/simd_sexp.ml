open! Core

external _extract_structural_indices
  :  Bigstring.t
  -> (int64, Bigarray.int64_elt, Bigarray.c_layout) Bigarray.Array1.t
  -> int64
  -> int64
  -> int64
  = "ml_extract_structural_indices"

let extract_structural_indices ~input ~output ~output_index ~start_offset =
  assert (Bigarray.Array1.dim output >= output_index + 64);
  _extract_structural_indices
    input
    output
    (Int64.of_int output_index)
    (Int64.of_int start_offset)
  |> Int64.to_int_exn
;;

module State = struct
  type t =
    { mutable stack : Sexp.t list list
    ; mutable previous_index_if_atom : [ `None | `Naked of int | `Quoted of int ]
    ; direct_emit : Sexp.t -> unit
    }

  let create ~direct_emit = { stack = []; previous_index_if_atom = `None; direct_emit }

  let process_escape_sequences input lo hi =
    let buffer = Buffer.create (hi - lo) in
    let escape_next = ref false in
    for i = lo to hi - 1 do
      if !escape_next
      then (
        Buffer.add_char buffer input.{i};
        escape_next := false)
      else (
        match input.{i} with
        | '\\' -> escape_next := true
        | _ -> Buffer.add_char buffer input.{i})
    done;
    if !escape_next then Buffer.add_char buffer '\\';
    Buffer.contents buffer
  ;;

  let emit_atom t input previous_index next_index =
    let the_atom =
      Sexp.Atom
        (Bigstring.To_string.sub
           input
           ~pos:previous_index
           ~len:(next_index - previous_index))
    in
    match t.stack with
    | [] -> t.direct_emit the_atom
    | stack_hd :: stack_tl -> t.stack <- (the_atom :: stack_hd) :: stack_tl
  ;;

  let emit_atom_quoted t input previous_index next_index =
    let the_atom = Sexp.Atom (process_escape_sequences input previous_index next_index) in
    match t.stack with
    | [] -> t.direct_emit the_atom
    | stack_hd :: stack_tl -> t.stack <- (the_atom :: stack_hd) :: stack_tl
  ;;

  let process t input next_index =
    let finalise_naked_atom () =
      match t.previous_index_if_atom with
      | `Naked previous_index ->
        emit_atom t input previous_index next_index;
        t.previous_index_if_atom <- `None
      | `Quoted previous_index when Char.O.(input.{next_index} <> '"') ->
        raise_s
          [%sexp
            "Invariant violated, open-quote can't be terminated by this structural char"
            , { previous_index : int; next_index : int }]
      | _ -> ()
    in
    match input.{next_index} with
    | '(' ->
      finalise_naked_atom ();
      t.stack <- [] :: t.stack
    | ')' ->
      finalise_naked_atom ();
      (match t.stack with
      | [] -> raise_s [%sexp "Too many closing parens"]
      | stack_hd :: stack_tl ->
        let the_sexp = Sexp.List (List.rev stack_hd) in
        (match stack_tl with
        | [] ->
          t.direct_emit the_sexp;
          t.stack <- stack_tl
        | stack_2nd_hd :: stack_2nd_tl ->
          t.stack <- (the_sexp :: stack_2nd_hd) :: stack_2nd_tl))
    | ' ' | '\t' | '\n' -> finalise_naked_atom ()
    | '"' ->
      (match t.previous_index_if_atom with
      | `None -> t.previous_index_if_atom <- `Quoted (next_index + 1)
      | `Naked previous_index ->
        emit_atom t input previous_index next_index;
        t.previous_index_if_atom <- `Quoted (next_index + 1)
      | `Quoted previous_index ->
        emit_atom_quoted t input previous_index next_index;
        t.previous_index_if_atom <- `None)
    | _ ->
      (match t.previous_index_if_atom with
      | `None -> t.previous_index_if_atom <- `Naked next_index
      | `Naked previous_index ->
        raise_s
          [%sexp
            "Invariant violated, two naked atom structural indices back-to-back"
            , { previous_index : int; next_index : int }]
      | `Quoted previous_index ->
        raise_s
          [%sexp
            "Invariant violated, naked atom structural index immediately following \
             open-quote atom structural index"
            , { previous_index : int; next_index : int }])
  ;;

  let process_eof t input =
    (match t.previous_index_if_atom with
    | `None -> ()
    | `Naked previous_index -> emit_atom t input previous_index (Bigstring.length input)
    | `Quoted previous_index ->
      raise_s [%sexp "Unterminated quote", { previous_index : int }]);
    match t.stack with
    | [] -> ()
    | _ :: _ ->
      raise_s [%sexp "Not enough closing parens before EOF", (t.stack : Sexp.t list list)]
  ;;
end

let run actual_string ~f =
  let actual_length = String.length actual_string in
  let input =
    Bigstring.of_string
      (actual_string ^ String.make ((64 - (actual_length mod 64)) mod 64) ' ')
  in
  assert (Bigstring.length input mod 64 = 0);
  let output = Bigarray.Array1.create Int64 C_layout (Bigstring.length input) in
  let n = extract_structural_indices ~input ~output ~output_index:0 ~start_offset:0 in
  let state = State.create ~direct_emit:(fun sexp -> f sexp) in
  for i = 0 to n - 1 do
    State.process state input (Int64.to_int_exn output.{i})
  done;
  State.process_eof state input
;;
