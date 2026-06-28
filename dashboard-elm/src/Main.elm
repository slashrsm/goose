port module Main exposing (main)

import Browser
import Html exposing (..)
import Html.Attributes exposing (..)
import Html.Events exposing (onClick)
import Http
import Json.Decode as D
import Svg
import Svg.Attributes as SA
import Time


-- MODEL


type Tab
    = Requests
    | Transactions
    | Scenarios
    | Errors


type Conn
    = Disconnected
    | Polling
    | Live


type alias Timeseries =
    { elapsedSecs : List Int
    , requestsPerSecond : List Float
    , averageResponseTimeMs : List Float
    , users : List Float
    }


type alias ErrorEntry =
    { message : String
    , count : Int
    }


type alias RequestRow =
    { method : String
    , name : String
    , successCount : Int
    , failCount : Int
    , requestsPerSecond : Float
    , failuresPerSecond : Float
    , responseTimeAverage : Float
    , responseTimeMinimum : Int
    , responseTimeMaximum : Int
    , p50 : Int
    , p95 : Int
    , p99 : Int
    }


type alias TransactionRow =
    { scenario : String
    , name : String
    , timesRun : Int
    , fails : Int
    , transactionsPerSecond : Float
    , failPerSecond : Float
    , responseTimeAverage : Float
    , responseTimeMinimum : Int
    , responseTimeMaximum : Int
    }


type alias ScenarioRow =
    { name : String
    , users : Int
    , timesRun : Int
    , scenariosPerSecond : Float
    , responseTimeAverage : Float
    , responseTimeMinimum : Int
    , responseTimeMaximum : Int
    }


type alias Summary =
    { phase : String
    , elapsedSecs : Int
    , activeUsers : Int
    , hosts : List String
    , totalRequests : Int
    , totalFailures : Int
    , requestsPerSecond : Float
    , failPercent : Float
    , p95 : Int
    , topErrors : List ErrorEntry
    , requests : List RequestRow
    , transactions : List TransactionRow
    , scenarios : List ScenarioRow
    , timeseries : Timeseries
    }


type alias Model =
    { summary : Summary
    , conn : Conn
    , tab : Tab
    }


emptyTimeseries : Timeseries
emptyTimeseries =
    { elapsedSecs = []
    , requestsPerSecond = []
    , averageResponseTimeMs = []
    , users = []
    }


emptySummary : Summary
emptySummary =
    { phase = "Idle"
    , elapsedSecs = 0
    , activeUsers = 0
    , hosts = []
    , totalRequests = 0
    , totalFailures = 0
    , requestsPerSecond = 0
    , failPercent = 0
    , p95 = 0
    , topErrors = []
    , requests = []
    , transactions = []
    , scenarios = []
    , timeseries = emptyTimeseries
    }


init : () -> ( Model, Cmd Msg )
init _ =
    ( { summary = emptySummary, conn = Disconnected, tab = Requests }
    , fetchSummary
    )


-- PORTS (SSE from bootstrap JS)


port sseSummary : (String -> msg) -> Sub msg


-- UPDATE


type Msg
    = Tick Time.Posix
    | GotSummary (Result Http.Error Summary)
    | SetTab Tab
    | SseSummary String


update : Msg -> Model -> ( Model, Cmd Msg )
update msg model =
    case msg of
        Tick _ ->
            -- Keep polling as fallback even when SSE is live (cheap; keeps UI moving if SSE stalls)
            ( model, fetchSummary )

        GotSummary (Ok s) ->
            let
                conn =
                    if model.conn == Live then
                        Live

                    else
                        Polling
            in
            ( { model | summary = s, conn = conn }, Cmd.none )

        GotSummary (Err _) ->
            ( { model | conn = Disconnected }, Cmd.none )

        SetTab t ->
            ( { model | tab = t }, Cmd.none )

        SseSummary raw ->
            case D.decodeString summaryDecoder raw of
                Ok s ->
                    ( { model | summary = s, conn = Live }, Cmd.none )

                Err _ ->
                    ( model, Cmd.none )


fetchSummary : Cmd Msg
fetchSummary =
    Http.get
        { url = "/api/v1/summary"
        , expect = Http.expectJson GotSummary summaryDecoder
        }


subscriptions : Model -> Sub Msg
subscriptions _ =
    Sub.batch
        [ Time.every 1000 Tick
        , sseSummary SseSummary
        ]


-- JSON (pipeline decoders)


dec : String -> D.Decoder a -> D.Decoder (a -> b) -> D.Decoder b
dec field decoder =
    D.map2 (|>) (D.field field decoder)


intAsFloat : D.Decoder Float
intAsFloat =
    D.oneOf [ D.float, D.map toFloat D.int ]


summaryDecoder : D.Decoder Summary
summaryDecoder =
    D.succeed Summary
        |> dec "phase" D.string
        |> dec "elapsed_secs" D.int
        |> dec "active_users" D.int
        |> dec "hosts" (D.list D.string)
        |> dec "total_requests" D.int
        |> dec "total_failures" D.int
        |> dec "requests_per_second" D.float
        |> dec "fail_percent" D.float
        |> dec "p95" D.int
        |> dec "top_errors" (D.list errorDecoder)
        |> dec "requests" (D.list requestDecoder)
        |> dec "transactions" (D.list transactionDecoder)
        |> dec "scenarios" (D.list scenarioDecoder)
        |> dec "timeseries" timeseriesDecoder


errorDecoder : D.Decoder ErrorEntry
errorDecoder =
    D.succeed ErrorEntry
        |> dec "message" D.string
        |> dec "count" D.int


requestDecoder : D.Decoder RequestRow
requestDecoder =
    D.succeed RequestRow
        |> dec "method" D.string
        |> dec "name" D.string
        |> dec "success_count" D.int
        |> dec "fail_count" D.int
        |> dec "requests_per_second" D.float
        |> dec "failures_per_second" D.float
        |> dec "response_time_average" D.float
        |> dec "response_time_minimum" D.int
        |> dec "response_time_maximum" D.int
        |> dec "p50" D.int
        |> dec "p95" D.int
        |> dec "p99" D.int


transactionDecoder : D.Decoder TransactionRow
transactionDecoder =
    D.succeed TransactionRow
        |> dec "scenario" D.string
        |> dec "name" D.string
        |> dec "times_run" D.int
        |> dec "fails" D.int
        |> dec "transactions_per_second" D.float
        |> dec "fail_per_second" D.float
        |> dec "response_time_average" D.float
        |> dec "response_time_minimum" D.int
        |> dec "response_time_maximum" D.int


scenarioDecoder : D.Decoder ScenarioRow
scenarioDecoder =
    D.succeed ScenarioRow
        |> dec "name" D.string
        |> dec "users" D.int
        |> dec "times_run" D.int
        |> dec "scenarios_per_second" D.float
        |> dec "response_time_average" D.float
        |> dec "response_time_minimum" D.int
        |> dec "response_time_maximum" D.int


timeseriesDecoder : D.Decoder Timeseries
timeseriesDecoder =
    D.succeed Timeseries
        |> dec "elapsed_secs" (D.list D.int)
        |> dec "requests_per_second" (D.list intAsFloat)
        |> dec "average_response_time_ms" (D.list D.float)
        |> dec "users" (D.list intAsFloat)


-- VIEW helpers


fmt2 : Float -> String
fmt2 n =
    String.fromFloat (toFloat (round (n * 100)) / 100)


phaseClass : String -> String
phaseClass phase =
    "badge phase-" ++ String.toLower phase


connClass : Conn -> String
connClass c =
    case c of
        Disconnected ->
            "conn disconnected"

        Polling ->
            "conn polling"

        Live ->
            "conn live"


connLabel : Conn -> String
connLabel c =
    case c of
        Disconnected ->
            "disconnected"

        Polling ->
            "polling"

        Live ->
            "live"


errorTotal : List ErrorEntry -> Int
errorTotal =
    List.foldl (\e acc -> acc + e.count) 0


lineChart : String -> String -> List Float -> List String -> Html msg
lineChart seriesName color values xLabels =
    let
        w =
            480

        h =
            220

        ml =
            48

        mr =
            16

        mt =
            28

        mb =
            36

        pw =
            w - ml - mr

        ph =
            h - mt - mb

        minV =
            0

        maxRaw =
            List.maximum values |> Maybe.withDefault 1

        maxV =
            if maxRaw <= minV then
                minV + 1

            else
                maxRaw * 1.05

        span =
            maxV - minV

        n =
            List.length values

        nf =
            toFloat (Basics.max 1 (n - 1))

        pts =
            List.indexedMap
                (\i v ->
                    ( ml + toFloat i / nf * pw
                    , mt + ph - (v - minV) / span * ph
                    )
                )
                values

        lineD =
            case pts of
                [] ->
                    ""

                ( x0, y0 ) :: rest ->
                    "M "
                        ++ String.fromFloat x0
                        ++ " "
                        ++ String.fromFloat y0
                        ++ String.concat
                            (List.map
                                (\( x, y ) ->
                                    " L " ++ String.fromFloat x ++ " " ++ String.fromFloat y
                                )
                                rest
                            )

        areaD =
            case pts of
                [] ->
                    ""

                ( x0, _ ) :: _ ->
                    let
                        lastX =
                            pts
                                |> List.reverse
                                |> List.head
                                |> Maybe.map Tuple.first
                                |> Maybe.withDefault x0
                    in
                    "M "
                        ++ String.fromFloat x0
                        ++ " "
                        ++ String.fromFloat (mt + ph)
                        ++ " L "
                        ++ String.dropLeft 2 lineD
                        ++ " L "
                        ++ String.fromFloat lastX
                        ++ " "
                        ++ String.fromFloat (mt + ph)
                        ++ " Z"

        yTicks =
            List.range 0 5
                |> List.map
                    (\t ->
                        let
                            frac =
                                toFloat t / 5

                            y =
                                mt + ph - frac * ph

                            val =
                                minV + frac * span
                        in
                        Svg.g []
                            [ Svg.line
                                [ SA.x1 (String.fromFloat ml)
                                , SA.y1 (String.fromFloat y)
                                , SA.x2 (String.fromFloat (ml + pw))
                                , SA.y2 (String.fromFloat y)
                                , SA.stroke "#2d3a4d"
                                , SA.strokeDasharray "4 4"
                                ]
                                []
                            , Svg.text_
                                [ SA.x (String.fromFloat (ml - 6))
                                , SA.y (String.fromFloat y)
                                , SA.textAnchor "end"
                                , SA.dominantBaseline "middle"
                                , SA.fill "#8b9bb4"
                                , SA.fontSize "10"
                                ]
                                [ Svg.text (fmt2 val) ]
                            ]
                    )

        xTickCount =
            Basics.min 6 (Basics.max 1 n)

        xTicks =
            List.range 0 (xTickCount - 1)
                |> List.map
                    (\t ->
                        let
                            idx =
                                if xTickCount == 1 then
                                    0

                                else
                                    t * (n - 1) // (xTickCount - 1)

                            x =
                                ml + toFloat idx / nf * pw

                            lab =
                                List.drop idx xLabels
                                    |> List.head
                                    |> Maybe.withDefault (String.fromInt idx)
                        in
                        Svg.text_
                            [ SA.x (String.fromFloat x)
                            , SA.y (String.fromFloat (h - 10))
                            , SA.textAnchor "middle"
                            , SA.fill "#8b9bb4"
                            , SA.fontSize "10"
                            ]
                            [ Svg.text lab ]
                    )
    in
    if List.isEmpty values then
        Svg.svg [ SA.class "spark", SA.viewBox ("0 0 " ++ String.fromFloat w ++ " " ++ String.fromFloat h) ]
            [ Svg.text_ [ SA.x "24", SA.y "110", SA.fill "#8b9bb4" ] [ Svg.text "no data yet" ] ]

    else
        Svg.svg [ SA.class "spark", SA.viewBox ("0 0 " ++ String.fromFloat w ++ " " ++ String.fromFloat h) ]
            ([ Svg.rect [ SA.x (String.fromFloat (w - 120)), SA.y "6", SA.width "12", SA.height "12", SA.fill color, SA.rx "2" ] []
             , Svg.text_ [ SA.x (String.fromFloat (w - 104)), SA.y "16", SA.fill "#8b9bb4", SA.fontSize "11" ] [ Svg.text seriesName ]
             ]
                ++ yTicks
                ++ [ Svg.line
                        [ SA.x1 (String.fromFloat ml)
                        , SA.y1 (String.fromFloat mt)
                        , SA.x2 (String.fromFloat ml)
                        , SA.y2 (String.fromFloat (mt + ph))
                        , SA.stroke "#2d3a4d"
                        , SA.strokeWidth "1.5"
                        ]
                        []
                   , Svg.line
                        [ SA.x1 (String.fromFloat ml)
                        , SA.y1 (String.fromFloat (mt + ph))
                        , SA.x2 (String.fromFloat (ml + pw))
                        , SA.y2 (String.fromFloat (mt + ph))
                        , SA.stroke "#2d3a4d"
                        , SA.strokeWidth "1.5"
                        ]
                        []
                   , Svg.path [ SA.d areaD, SA.fill color, SA.opacity "0.2" ] []
                   , Svg.path [ SA.d lineD, SA.fill "none", SA.stroke color, SA.strokeWidth "2" ] []
                   ]
                ++ xTicks
                ++ [ Svg.text_
                        [ SA.x (String.fromFloat (ml + pw / 2))
                        , SA.y (String.fromFloat h)
                        , SA.textAnchor "middle"
                        , SA.fill "#8b9bb4"
                        , SA.fontSize "10"
                        ]
                        [ Svg.text "elapsed (s)" ]
                   ]
            )


view : Model -> Html Msg
view model =
    let
        s =
            model.summary

        xs =
            List.map String.fromInt s.timeseries.elapsedSecs
    in
    div []
        [ header [ class "top" ]
            [ div [ class "brand" ]
                [ span [ class "logo" ] [ text "Goose" ]
                , span [ class "subtitle" ] [ text "Live load test dashboard (Elm)" ]
                ]
            , div [ class "meta" ]
                [ span [ class (phaseClass s.phase) ] [ text s.phase ]
                , span [ class "mono" ] [ text (String.fromInt s.elapsedSecs ++ "s") ]
                , span [ class "hosts" ] [ text (String.join ", " s.hosts) ]
                , span [ class (connClass model.conn) ] [ text (connLabel model.conn) ]
                ]
            ]
        , section [ class "kpis" ]
            [ kpi "Active users" (String.fromInt s.activeUsers)
            , kpi "RPS" (fmt2 s.requestsPerSecond)
            , kpi "Fail %" (fmt2 s.failPercent ++ "%")
            , kpi "p95 (ms)" (String.fromInt s.p95)
            , kpi "Requests" (String.fromInt s.totalRequests)
            , kpi "Errors" (String.fromInt (errorTotal s.topErrors))
            ]
        , section [ class "charts" ]
            [ chartCard "Requests / second" (lineChart "RPS" "#3d9cf0" s.timeseries.requestsPerSecond xs)
            , chartCard "Active users" (lineChart "Users" "#3dd68c" s.timeseries.users xs)
            , chartCard "Avg response time (ms)" (lineChart "Avg RT (ms)" "#f5a524" s.timeseries.averageResponseTimeMs xs)
            ]
        , nav [ class "tabs" ]
            [ tabBtn model.tab Requests "Requests"
            , tabBtn model.tab Transactions "Transactions"
            , tabBtn model.tab Scenarios "Scenarios"
            , tabBtn model.tab Errors "Errors"
            ]
        , section [ class "panels" ] [ viewTab model ]
        , footer []
            [ text "Running metrics are approximate. Frontend: Elm 0.19. Use "
            , code [] [ text "--report-file" ]
            , text " for an archival HTML report when the test finishes."
            ]
        ]


kpi : String -> String -> Html msg
kpi label value =
    div [ class "kpi" ]
        [ div [ class "label" ] [ text label ]
        , div [ class "value" ] [ text value ]
        ]


chartCard : String -> Html msg -> Html msg
chartCard title chart =
    div [ class "chart-card" ]
        [ div [ class "chart-title" ] [ text title ]
        , chart
        ]


tabBtn : Tab -> Tab -> String -> Html Msg
tabBtn active t label =
    button
        [ class
            (if active == t then
                "tab active"

             else
                "tab"
            )
        , onClick (SetTab t)
        ]
        [ text label ]


viewTab : Model -> Html Msg
viewTab model =
    case model.tab of
        Requests ->
            div [ class "panel active" ]
                [ table []
                    [ thead []
                        [ tr []
                            [ th [] [ text "Method" ]
                            , th [] [ text "Name" ]
                            , th [] [ text "# Req" ]
                            , th [] [ text "# Fail" ]
                            , th [] [ text "RPS" ]
                            , th [] [ text "Fail/s" ]
                            , th [] [ text "Avg" ]
                            , th [] [ text "Min" ]
                            , th [] [ text "Max" ]
                            , th [] [ text "p50" ]
                            , th [] [ text "p95" ]
                            , th [] [ text "p99" ]
                            ]
                        ]
                    , tbody [] (List.map requestRow model.summary.requests)
                    ]
                ]

        Transactions ->
            div [ class "panel active" ]
                [ table []
                    [ thead []
                        [ tr []
                            [ th [] [ text "Scenario" ]
                            , th [] [ text "Transaction" ]
                            , th [] [ text "# Run" ]
                            , th [] [ text "# Fail" ]
                            , th [] [ text "TPS" ]
                            , th [] [ text "Fail/s" ]
                            , th [] [ text "Avg" ]
                            , th [] [ text "Min" ]
                            , th [] [ text "Max" ]
                            ]
                        ]
                    , tbody [] (List.map transactionRow model.summary.transactions)
                    ]
                ]

        Scenarios ->
            div [ class "panel active" ]
                [ table []
                    [ thead []
                        [ tr []
                            [ th [] [ text "Scenario" ]
                            , th [] [ text "Users" ]
                            , th [] [ text "# Run" ]
                            , th [] [ text "Scen/s" ]
                            , th [] [ text "Avg" ]
                            , th [] [ text "Min" ]
                            , th [] [ text "Max" ]
                            ]
                        ]
                    , tbody [] (List.map scenarioRow model.summary.scenarios)
                    ]
                ]

        Errors ->
            div [ class "panel active" ]
                [ table []
                    [ thead [] [ tr [] [ th [] [ text "Count" ], th [] [ text "Error" ] ] ]
                    , tbody []
                        (List.map
                            (\e ->
                                tr []
                                    [ td [] [ text (String.fromInt e.count) ]
                                    , td [ style "white-space" "normal" ] [ text e.message ]
                                    ]
                            )
                            model.summary.topErrors
                        )
                    ]
                ]


requestRow : RequestRow -> Html msg
requestRow r =
    tr []
        [ td [] [ text r.method ]
        , td [] [ text r.name ]
        , td [] [ text (String.fromInt (r.successCount + r.failCount)) ]
        , td [] [ text (String.fromInt r.failCount) ]
        , td [] [ text (fmt2 r.requestsPerSecond) ]
        , td [] [ text (fmt2 r.failuresPerSecond) ]
        , td [] [ text (fmt2 r.responseTimeAverage) ]
        , td [] [ text (String.fromInt r.responseTimeMinimum) ]
        , td [] [ text (String.fromInt r.responseTimeMaximum) ]
        , td [] [ text (String.fromInt r.p50) ]
        , td [] [ text (String.fromInt r.p95) ]
        , td [] [ text (String.fromInt r.p99) ]
        ]


transactionRow : TransactionRow -> Html msg
transactionRow t =
    tr []
        [ td [] [ text t.scenario ]
        , td [] [ text t.name ]
        , td [] [ text (String.fromInt t.timesRun) ]
        , td [] [ text (String.fromInt t.fails) ]
        , td [] [ text (fmt2 t.transactionsPerSecond) ]
        , td [] [ text (fmt2 t.failPerSecond) ]
        , td [] [ text (fmt2 t.responseTimeAverage) ]
        , td [] [ text (String.fromInt t.responseTimeMinimum) ]
        , td [] [ text (String.fromInt t.responseTimeMaximum) ]
        ]


scenarioRow : ScenarioRow -> Html msg
scenarioRow s =
    tr []
        [ td [] [ text s.name ]
        , td [] [ text (String.fromInt s.users) ]
        , td [] [ text (String.fromInt s.timesRun) ]
        , td [] [ text (fmt2 s.scenariosPerSecond) ]
        , td [] [ text (fmt2 s.responseTimeAverage) ]
        , td [] [ text (String.fromInt s.responseTimeMinimum) ]
        , td [] [ text (String.fromInt s.responseTimeMaximum) ]
        ]


main : Program () Model Msg
main =
    Browser.element
        { init = init
        , update = update
        , subscriptions = subscriptions
        , view = view
        }
