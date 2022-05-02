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

external _unescape : Bigstring.t -> Bigstring.t -> int64 option = "ml_unescape"

let unescape ~input ~output =
  assert (Bigstring.length output >= Bigstring.length input);
  _unescape input output |> Option.map ~f:Int64.to_int_exn
;;

module State = struct
  type t =
    { mutable stack : Sexp.t list list
    ; direct_emit : Sexp.t -> unit
    ; mutable atom_buffer : Bigstring.t
    }

  let create ~direct_emit =
    { stack = []; direct_emit; atom_buffer = Bigstring.create 128 }
  ;;

  let process_escape_sequences t input lo hi =
    let input = Bigstring.sub_shared input ~pos:lo ~len:(hi - lo) in
    if Bigstring.length t.atom_buffer < hi - lo
    then (
      let rec new_length len = if len >= hi - lo then len else new_length (len * 2) in
      t.atom_buffer
        <- Bigstring.unsafe_destroy_and_resize
             t.atom_buffer
             ~len:(new_length (Bigstring.length t.atom_buffer)));
    match unescape ~input ~output:t.atom_buffer with
    | None -> raise_s [%sexp "Invalid escape sequence", (input : Bigstring.t)]
    | Some len -> Bigstring.to_string t.atom_buffer ~len
  ;;

  let emit_atom t input previous_index next_index =
    let the_atom =
      Sexp.Atom
        (Bigstring.to_string input ~pos:previous_index ~len:(next_index - previous_index))
    in
    match t.stack with
    | [] -> t.direct_emit the_atom
    | stack_hd :: stack_tl -> t.stack <- (the_atom :: stack_hd) :: stack_tl
  ;;

  let emit_atom_quoted t input previous_index next_index =
    let the_atom =
      Sexp.Atom (process_escape_sequences t input previous_index next_index)
    in
    match t.stack with
    | [] -> t.direct_emit the_atom
    | stack_hd :: stack_tl -> t.stack <- (the_atom :: stack_hd) :: stack_tl
  ;;

  let process_one t input indices i =
    let[@inline always] index i =
      Int64.to_int_trunc (Bigarray.Array1.unsafe_get indices i)
    in
    match input.{index i} with
    | '(' ->
      t.stack <- [] :: t.stack;
      i + 1
    | ')' ->
      (match t.stack with
      | [] -> raise_s [%sexp "Too many closing parens"]
      | stack_hd :: stack_tl ->
        let the_sexp = Sexp.List (List.rev stack_hd) in
        (match stack_tl with
        | [] ->
          t.direct_emit the_sexp;
          t.stack <- stack_tl
        | stack_2nd_hd :: stack_2nd_tl ->
          t.stack <- (the_sexp :: stack_2nd_hd) :: stack_2nd_tl));
      i + 1
    | ' ' | '\t' | '\n' -> i + 1
    | '"' ->
      assert (Char.equal input.{index (i + 1)} '"');
      emit_atom_quoted t input (index i + 1) (index (i + 1));
      i + 2
    | _ ->
      emit_atom t input (index i) (index (i + 1));
      i + 1
  ;;

  let process_eof t =
    match t.stack with
    | [] -> ()
    | _ :: _ ->
      raise_s [%sexp "Not enough closing parens before EOF", (t.stack : Sexp.t list list)]
  ;;

  let process_all t input indices =
    let rec loop i =
      if i >= Bigarray.Array1.dim indices
      then process_eof t
      else loop (process_one t input indices i)
    in
    loop 0
  ;;
end

let run actual_string ~f =
  let actual_length = String.length actual_string in
  let input =
    Bigstring.of_string
      (actual_string ^ String.make ((64 - (actual_length mod 64)) mod 64) ' ')
  in
  assert (Bigstring.length input mod 64 = 0);
  let indices = Bigarray.Array1.create Int64 C_layout (Bigstring.length input) in
  let n_indices =
    extract_structural_indices ~input ~output:indices ~output_index:0 ~start_offset:0
  in
  let indices = Bigarray.Array1.sub indices 0 n_indices in
  let state = State.create ~direct_emit:(fun sexp -> f sexp) in
  State.process_all state input indices
;;

let of_string_many string =
  let rev_result = ref [] in
  run string ~f:(fun sexp -> rev_result := sexp :: !rev_result);
  List.rev !rev_result
;;
