open! Core

let profile ~filename =
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

let cmd_profile_test =
  Command.basic
    ~summary:"Parse a sexp repeatedly using various functions"
    (let%map_open.Command filename = anon ("FILENAME" %: Filename_unix.arg_type) in
     fun () ->
       let rec loop () =
         profile ~filename;
         loop ()
       in
       loop ())
;;

let cmd_multi_select =
  Command.basic
    ~summary:"Select multiple field names"
    (let%map_open.Command select_keys =
       anon (non_empty_sequence_as_list ("KEY" %: string))
     and assume_machine_input =
       flag
         "-assume-machine-input"
         no_arg
         ~doc:
           " match keys only if they appear literally in machine format (faster, but not \
            necessarily true for hand-written sexps)"
     and output_kind =
       choose_one
         ~if_nothing_chosen:(Default_to `Values)
         [ flag "-values" no_arg ~doc:" output values of matching keys (default)"
           |> map ~f:(fun b -> Option.some_if b `Values)
         ; flag
             "-labeled"
             no_arg
             ~doc:" output values of matching keys labelled with the key that matches it"
           |> map ~f:(fun b -> Option.some_if b `Labeled)
         ; flag
             "-csv"
             no_arg
             ~doc:" output values of matching keys in CSV format, with keys as headers"
           |> map ~f:(fun b -> Option.some_if b `Labeled)
         ]
     in
     fun () ->
       Simd_sexp.Select.multi_select ~select_keys ~assume_machine_input ~output_kind)
;;

let command =
  Command.group
    ~summary:"sexp tool"
    [ "profile-test", cmd_profile_test; "multi-select", cmd_multi_select ]
;;

let () = Command_unix.run command
