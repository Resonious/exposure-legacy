# frozen_string_literal: true

module Exposure
  # Light wrapper around the Core Trace
  class Trace
    def initialize
      @core = Core.new_trace

      @trace_points = []
      @trace_points << TracePoint.new(:b_call, :class, :call, &method(:push))
      @trace_points << TracePoint.new(:return, :b_return, :end, &method(:pop))
    end

    def start
      @trace_points.each(&:enable)
    end

    def stop
      @trace_points.each(&:disable)
    end

    def push(trace)
      calla = caller_locations(2..2).first
      receiver = trace.binding.receiver if trace.binding.receiver.is_a?(Class)
      klass = trace.defined_class

      # First push
      Core.push_frame(
        @core,

        trace.event,

        calla.path,
        calla.lineno,

        trace.path,
        trace.lineno,

        (klass.name || klass.to_s if klass),
        trace.method_id.to_s,

        (receiver.name || receiver.to_s if receiver)
      )

      # Then add locals
      add_locals(trace.binding)
    end

    def pop(trace)
      if trace.event == :return || trace.event == :b_return
        return_class = trace.return_value.class
        return_type = return_class.name || return_class.to_s
      end

      add_locals(trace.binding)
      Core.pop_frame(@core, return_type)
    end

    private

    def add_locals(frame_binding)
      frame_binding.local_variables.each do |var|
        begin
          val = frame_binding.local_variable_get(var)
          Core.add_local(@core, var.to_s, val.class.name || val.class.to_s)
        rescue StandardError => e
          Core.add_local(@core, var.to_s, "((#{e.class} during inspect))")
        end
      end
    end
  end
end
