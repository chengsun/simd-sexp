open! Core

external _extract_structural_indices
  :  string
  -> int array
  -> int
  -> int
  -> int
  = "ml_extract_structural_indices"
  [@@noalloc]

let extract_structural_indices ~input ~output ~output_index ~start_offset =
  assert (Array.length output >= output_index + 64);
  _extract_structural_indices input output output_index start_offset
;;

external _unescape : string -> int -> int -> bytes -> int = "ml_unescape" [@@noalloc]

let unescape ~input ~pos ~len ~output =
  assert (Bytes.length output >= len);
  match _unescape input pos len output with
  | -1 -> None
  | output_len -> Some output_len
;;

module State = struct
  type t =
    { mutable stack : Sexp.t list list
    ; direct_emit : Sexp.t -> unit
    ; mutable atom_buffer : bytes
    }

  let create ~direct_emit = { stack = []; direct_emit; atom_buffer = Bytes.create 128 }

  let process_escape_sequences t input lo hi =
    let atom_buffer =
      if Bytes.length t.atom_buffer < hi - lo
      then (
        let rec new_length len = if len >= hi - lo then len else new_length (len * 2) in
        let new_buffer = Bytes.create (new_length (2 * Bytes.length t.atom_buffer)) in
        t.atom_buffer <- new_buffer;
        new_buffer)
      else t.atom_buffer
    in
    match unescape ~input ~pos:lo ~len:(hi - lo) ~output:atom_buffer with
    | None -> raise_s [%sexp "Invalid escape sequence"]
    | Some len -> Bytes.To_string.sub ~pos:0 ~len atom_buffer
  ;;

  let emit_atom t input previous_index next_index =
    let the_atom =
      Sexp.Atom (String.sub input ~pos:previous_index ~len:(next_index - previous_index))
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

  let process_one t ~input ~indices ~indices_len ~i =
    let[@inline always] index i =
      assert (0 <= i && i < indices_len);
      Array.unsafe_get indices i
    in
    match String.unsafe_get input (index i) with
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
      assert (Char.equal (String.unsafe_get input (index (i + 1))) '"');
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

  let process_all t ~input ~indices ~indices_len =
    let rec loop i =
      if i >= indices_len
      then process_eof t
      else loop (process_one t ~input ~indices ~indices_len ~i)
    in
    loop 0
  ;;
end

let run actual_string ~f =
  let actual_length = String.length actual_string in
  let input = actual_string ^ String.make ((64 - (actual_length mod 64)) mod 64) ' ' in
  assert (String.length input mod 64 = 0);
  let indices = Array.create 0 ~len:(String.length input) in
  let indices_len =
    extract_structural_indices ~input ~output:indices ~output_index:0 ~start_offset:0
  in
  let state = State.create ~direct_emit:(fun sexp -> f sexp) in
  State.process_all state ~input ~indices ~indices_len
;;

let of_string_many string =
  let rev_result = ref [] in
  run string ~f:(fun sexp -> rev_result := sexp :: !rev_result);
  List.rev !rev_result
;;
