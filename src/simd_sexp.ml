open! Core

module Extract_structural_indices = struct
  type t

  external create : unit -> t = "ml_extract_structural_indices_create_state"

  external _run
    :  t
    -> string
    -> int
    -> (int32, Bigarray.int32_elt, Bigarray.c_layout) Bigarray.Array1.t
    -> int
    -> int * int
    = "ml_extract_structural_indices"

  let run t ~input ~input_index ~indices ~indices_index =
    _run t input input_index indices indices_index
  ;;
end

external _unescape : string -> int -> int -> string option = "ml_unescape"

let unescape ~input ~pos ~len = _unescape input pos len

module Stack = struct
  type t =
    | Nil
    | Sexp of Sexp.t * t
    | Open of t
  [@@deriving sexp_of]
end

module State = struct
  type t =
    { mutable atom_buffer : bytes
    ; indices_buffer : (int32, Bigarray.int32_elt, Bigarray.c_layout) Bigarray.Array1.t
    ; extract_structural_indices : Extract_structural_indices.t
    }

  let create () =
    { atom_buffer = Bytes.create 128
    ; indices_buffer = Bigarray.Array1.create Bigarray.int32 Bigarray.c_layout 512
    ; extract_structural_indices = Extract_structural_indices.create ()
    }
  ;;

  let process_escape_sequences (_ : t) input lo hi =
    match unescape ~input ~pos:lo ~len:(hi - lo) with
    | None -> raise_s [%sexp "Invalid escape sequence"]
    | Some s -> s
  ;;

  let emit_atom (_ : t) stack input previous_index next_index =
    let the_atom =
      Sexp.Atom (String.sub input ~pos:previous_index ~len:(next_index - previous_index))
    in
    Stack.Sexp (the_atom, stack)
  ;;

  let emit_atom_quoted t stack input previous_index next_index =
    let the_atom =
      Sexp.Atom (process_escape_sequences t input previous_index next_index)
    in
    Stack.Sexp (the_atom, stack)
  ;;

  let emit_closing stack =
    let rec gather accum = function
      | Stack.Nil -> raise_s [%sexp "Too many closing parens"]
      | Stack.Sexp (sexp, stack) -> gather (sexp :: accum) stack
      | Stack.Open stack -> Stack.Sexp (Sexp.List accum, stack)
    in
    gather [] stack
  ;;

  let emit_eof stack =
    let rec gather accum = function
      | Stack.Nil -> accum
      | Stack.Sexp (sexp, stack) -> gather (sexp :: accum) stack
      | Stack.Open _ -> raise_s [%sexp "Unmatched open paren"]
    in
    gather [] stack
  ;;

  let process_one t stack ~input ~indices_index ~indices_len:(_ : int) =
    let[@inline always] index i =
      Bigarray.Array1.unsafe_get t.indices_buffer i |> Int32.to_int_trunc
    in
    let this_index = index indices_index in
    match String.unsafe_get input this_index with
    | '(' -> Stack.Open stack, indices_index + 1
    | ')' -> emit_closing stack, indices_index + 1
    | ' ' | '\t' | '\n' -> stack, indices_index + 1
    | '"' ->
      ( emit_atom_quoted t stack input (this_index + 1) (index (indices_index + 1))
      , indices_index + 2 )
    | _ ->
      emit_atom t stack input this_index (index (indices_index + 1)), indices_index + 1
  ;;

  let process_eof (_ : t) stack = emit_eof stack

  let process_all t ~input =
    let rec loop ~stack ~input_index ~indices_index ~indices_len =
      let input_index, indices_index, indices_len =
        if indices_index + 2 <= indices_len || input_index >= String.length input
        then input_index, indices_index, indices_len
        else (
          let n_unconsumed_indices = indices_len - indices_index in
          for i = 0 to n_unconsumed_indices - 1 do
            Bigarray.Array1.unsafe_set
              t.indices_buffer
              i
              (Bigarray.Array1.unsafe_get t.indices_buffer (indices_index + i))
          done;
          let input_index', indices_len =
            Extract_structural_indices.run
              t.extract_structural_indices
              ~input
              ~input_index
              ~indices:t.indices_buffer
              ~indices_index:n_unconsumed_indices
          in
          input_index', 0, indices_len)
      in
      if indices_index >= indices_len
      then (
        assert (input_index = String.length input);
        process_eof t stack)
      else (
        let stack, indices_index =
          process_one t stack ~input ~indices_index ~indices_len
        in
        loop ~stack ~input_index ~indices_index ~indices_len)
    in
    loop ~stack:Nil ~input_index:0 ~indices_index:0 ~indices_len:0
  ;;
end

let of_string_many actual_string =
  let actual_length = String.length actual_string in
  let input = actual_string ^ String.make ((64 - (actual_length mod 64)) mod 64) ' ' in
  assert (String.length input mod 64 = 0);
  let state = State.create () in
  State.process_all state ~input
;;
