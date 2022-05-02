open! Core

let parse ~filename =
  let file_contents = In_channel.read_all filename in
  let start_time = Time_ns.now () in
  let sexps_from_core_sexp = Sexp.of_string_many file_contents in
  let end_time = Time_ns.now () in
  printf !"Core.Sexp elapsed: %{Time_ns.Span#hum}\n%!" (Time_ns.diff end_time start_time);
  let start_time = Time_ns.now () in
  let sexps_from_simd_sexp = Simd_sexp.of_string_many file_contents in
  let end_time = Time_ns.now () in
  printf !"Simd_sexp elapsed: %{Time_ns.Span#hum}\n%!" (Time_ns.diff end_time start_time);
  let rec assert_sexp_equality (a : Sexp.t) (b : Sexp.t) =
    match a, b with
    | Atom a, Atom b ->
      if String.equal a b
      then ()
      else raise_s [%sexp "differing atoms", (a : string), (b : string)]
    | List la, List lb ->
      (match List.zip la lb with
      | Ok l -> List.iter l ~f:(fun (a, b) -> assert_sexp_equality a b)
      | Unequal_lengths ->
        raise_s
          [%sexp
            "list with differing lengths", (List.length la : int), (List.length lb : int)])
    | _ -> raise_s [%sexp "one is atom other is list"]
  in
  assert_sexp_equality (Sexp.List sexps_from_core_sexp) (Sexp.List sexps_from_simd_sexp)
;;

let command =
  Command.basic
    ~summary:"Parse a sexp"
    (let%map_open.Command filename = anon ("FILENAME" %: Filename_unix.arg_type) in
     fun () ->
       let rec loop () =
         parse ~filename;
         loop ()
       in
       loop ())
;;

let () = Command_unix.run command
